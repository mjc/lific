(() => {
  'use strict';

  const routeHref = (route, win = globalThis) => route.startsWith('/') && !route.startsWith('//') ? (win.LificTopcoatRouting?.href(route) ?? route) : route;
  const $ = (root, selector) => root.querySelector(selector);
  const text = (doc, tag, value, className) => {
    const node = doc.createElement(tag);
    node.textContent = value == null ? '' : String(value);
    if (className) node.className = className;
    return node;
  };
  const identity = session => session?.state?.publicProject?.toUpperCase() || null;
  const projectName = value => /^[A-Za-z][A-Za-z0-9_-]*$/.test(value || '');
  const commentCursor = comments => comments.length ? comments[0] : null;
  const issueUrl = (project, identifier) => `/public/${encodeURIComponent(project)}/issues/${encodeURIComponent(identifier)}`;
  const pageUrl = (project, id) => `/public/${encodeURIComponent(project)}/pages/${id}`;
  const STATUSES = ['backlog', 'todo', 'active', 'done', 'cancelled'];
  const PRIORITIES = ['urgent', 'high', 'medium', 'low', 'none'];
  function publicHref(value, project, win = globalThis) {
    if (/^https?:\/\//i.test(value)) return value;
    if (/^#(?:comment-[1-9]\d*|att[1-9]\d*-L[1-9]\d*(?:-[1-9]\d*)?)$/.test(value)) return value;
    const route = value.startsWith('#/') ? value.slice(1) : value;
    const match = route.match(/^\/(?:public\/)?([A-Za-z][A-Za-z0-9_-]*)(\/[^?#]*)?([?#].*)?$/);
    if (!match || match[1].toUpperCase() !== project.toUpperCase()) return null;
    const path = match[2] || '/issues';
    if (!/^\/(?:issues|board|pages)$/.test(path) && !/^\/issues\/[A-Za-z][A-Za-z0-9_-]*-\d+$/.test(path) && !/^\/pages\/\d+$/.test(path)) return null;
    return routeHref(`/public/${encodeURIComponent(project)}${path}${match[3] || ''}`, win);
  }
  function attachmentTarget(value) {
    const match = String(value || '').match(/^#?att([1-9]\d*)-L([1-9]\d*)(?:-([1-9]\d*))?$/);
    if (!match) return null;
    const [id, from, to] = [Number(match[1]), Number(match[2]), Number(match[3] || match[2])];
    return [id, from, to].every(Number.isSafeInteger) ? {id, start: Math.min(from, to), end: Math.max(from, to)} : null;
  }
  // Match the existing issue search scorer and its bounded preview haystack.
  function searchScore(query, source) {
    const q = query.toLowerCase(), value = String(source || '').toLowerCase(), direct = value.indexOf(q);
    const boundary = /[\s\-_/.,()[\]{}<>:;!?"'`]/;
    if (direct !== -1) return direct === 0 ? .95 : boundary.test(value[direct - 1]) ? .9 : .8;
    let index = 0, first = -1, last = -1, run = 0, longest = 0, words = 0;
    for (let at = 0; index < q.length && at < value.length; at++) if (q[index] === value[at]) {
      if (first === -1) first = at;
      if (at === last + 1) run++; else {run = 1; if (boundary.test(at > 0 ? value[at - 1] : ' ')) words++;}
      longest = Math.max(longest, run); last = at; index++;
    }
    return index < q.length ? 0 : Math.min(.7, q.length / Math.max(1, last - first + 1) * .4 + longest / q.length * .4 + words / q.length * .2);
  }
  const publicIssue = issue => ({
    title: issue.title || 'Untitled issue', identifier: issue.identifier || '', status: issue.status || '',
    priority: issue.priority || '', description: issue.description || issue.body || '', id: issue.id,
  });

  function appendReferences(doc, parent, value, project) {
    const pattern = /\b([A-Z][A-Z0-9]{1,4})-(DOC-|PLAN-)?(\d+)(?:#comment-(\d+))?|(?<![A-Za-z0-9_&-])#([1-9]\d*)\b/g;
    let cursor = 0, match;
    while ((match = pattern.exec(value))) {
      parent.append(doc.createTextNode(value.slice(cursor, match.index)));
      let href = null;
      if (match[5]) href = `#comment-${match[5]}`;
      else if (match[1].toUpperCase() === project.toUpperCase() && match[2] !== 'PLAN-') {
        href = match[2] === 'DOC-' ? `/public/${project}/pages` : issueUrl(project, `${match[1]}-${match[3]}`);
        if (match[4] && !match[2]) href += `?comment=${match[4]}`;
      }
      if (href) { const link = text(doc, 'a', match[0]); link.href = routeHref(href, doc.defaultView); parent.append(link); }
      else parent.append(doc.createTextNode(match[0]));
      cursor = pattern.lastIndex;
    }
    parent.append(doc.createTextNode(value.slice(cursor)));
  }

  function renderMarkdown(doc, target, source, project) {
    const win = doc.defaultView;
    const diagrams = [];
    const renderer = new win.marked.Renderer();
    const code = renderer.code.bind(renderer);
    renderer.code = token => {
      if (token.lang !== 'mermaid') return code(token);
      const index = diagrams.push(token.text) - 1;
      return `<div data-public-diagram-index="${index}">Rendering diagram…</div>`;
    };
    const purifier = win.DOMPurify(win);
    purifier.addHook('uponSanitizeAttribute', (node, attribute) => {
      if (attribute.attrName !== 'src') return;
      const image = node.nodeName === 'IMG' && attribute.attrValue.match(/^\/(?:api\/)?attachments\/(\d+)(?:\/thumbnail)?\/?$/);
      if (image) node.setAttribute('data-public-image', image[1]);
      attribute.keepAttr = false;
    });
    const html = win.marked.parse(String(source ?? '').replace(/\r\n?/g, '\n'), {breaks:true, gfm:true, renderer});
    const fragment = purifier.sanitize(html, {
      RETURN_DOM_FRAGMENT:true,
      FORBID_TAGS:['video','audio','source','track','picture','iframe','object','embed','style','link','form','svg','image','use','math'],
      FORBID_ATTR:['style','srcset','poster','background','ping'],
    });
    for (const link of fragment.querySelectorAll('a')) {
      const value = link.getAttribute('href') || '';
      const attachment = value.match(/^\/(?:api\/)?attachments\/(\d+)(?:\/thumbnail)?\/?$/);
      const safe = publicHref(value, project, win);
      if (attachment) {link.href = '#download'; link.dataset.publicDownload = attachment[1];}
      else if (safe) {link.href = safe; link.rel = 'nofollow noopener';}
      else link.replaceWith(...link.childNodes);
    }
    for (const image of fragment.querySelectorAll('img')) {
      if (/^\d+$/.test(image.dataset.publicImage || '')) image.loading = 'lazy';
      else image.replaceWith(doc.createTextNode(image.alt));
    }
    const walker = doc.createTreeWalker(fragment, win.NodeFilter.SHOW_TEXT);
    const prose = [];
    while (walker.nextNode()) if (!walker.currentNode.parentElement?.closest('a,code,pre,[data-public-diagram-index]')) prose.push(walker.currentNode);
    for (const node of prose) {
      const linked = doc.createDocumentFragment();
      appendReferences(doc, linked, node.textContent, project);
      node.replaceWith(linked);
    }
    target.replaceChildren(fragment);
    const diagramTooComplex = value => {
      if (new TextEncoder().encode(value).length > 4096 || value.split(/[;\n]/).length + (value.match(/-->|==>|-\.->|---|->/g)?.length || 0) > 128) return true;
      const statements = value.replace(/"[^"]*"|%%[^\r\n]*/g, '').split(/[;\r\n]/).map(part => part.trim());
      const number = '[-+]?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:e[-+]?\\d+)?';
      const axis = new RegExp(`^x-axis\\b.*?(${number})\\s*-->\\s*(${number})\\s*$`, 'is');
      if (statements.some(part => /^xychart(?:-beta)?\b/i.test(part)) && statements.some(part => {const range = part.match(axis); return range && Number(range[1]) === Number(range[2]);})) return true;
      if (statements.some(part => /^radar-beta\b/i.test(part)) && statements.some(part => {const ticks = part.match(/^ticks\s+(\d+)\s*$/i); return ticks && Number(ticks[1]) > 128;})) return true;
      return statements.some(part => /^architecture-beta\b/i.test(part)) && statements.some(part => /^group\s+(?:__proto__|prototype|constructor)\b/i.test(part));
    };
    let diagramCount = 0;
    for (const block of target.querySelectorAll('[data-public-diagram-index]')) {
      const value = diagrams[Number(block.dataset.publicDiagramIndex)];
      delete block.dataset.publicDiagramIndex;
      if (typeof value !== 'string') {block.remove(); continue;}
      block.dataset.publicDiagram = '';
      // Match the legacy per-document budgets and keep image-bearing source
      // out of Mermaid's temporary DOM before SVG sanitization occurs.
      if (diagramTooComplex(value) || /(?:img|image)\s*:|<\s*(?:img|image|foreignObject)|url\s*\(|@import|%%\{/i.test(value)) {
        block.textContent = 'Mermaid diagram skipped: source is too complex or contains unsupported media.';
      } else if (++diagramCount > 2) block.textContent = 'Mermaid diagram skipped: this document contains too many diagrams.';
      else if (!win?.mermaid || !win?.DOMPurify) block.textContent = 'Diagram renderer unavailable.';
      else {
        win.mermaid.initialize({startOnLoad:false, securityLevel:'strict', suppressErrorRendering:true, htmlLabels:false, flowchart:{htmlLabels:false}, theme:doc.documentElement.classList.contains('dark')?'dark':'default', secure:['securityLevel','htmlLabels','startOnLoad','maxTextSize','maxEdges','suppressErrorRendering'], maxTextSize:4096, maxEdges:128});
        const id = `public-diagram-${Math.random().toString(36).slice(2)}`;
        void win.mermaid.render(id, value).then(result => {
          if (!block.isConnected) return;
          const svg = win.DOMPurify.sanitize(result.svg, {RETURN_DOM_FRAGMENT:true, USE_PROFILES:{svg:true,svgFilters:true}, FORBID_TAGS:['image','foreignObject','use','script','a'], FORBID_ATTR:['href','xlink:href','style']});
          // SVG presentation attributes and stylesheet text can fetch too.
          // Sanitize the detached fragment before attaching it to the page.
          const external = value => /@import|\\/i.test(value) || [...value.matchAll(/url\s*\(([^)]*)\)/gi)].some(match => !/^#[A-Za-z0-9_-]+$/.test(match[1].trim().replace(/^['"]|['"]$/g, '')));
          for (const style of svg.querySelectorAll('style')) if (external(style.textContent)) style.remove();
          for (const node of svg.querySelectorAll('*')) for (const attribute of [...node.attributes]) if (external(attribute.value)) node.removeAttribute(attribute.name);
          block.replaceChildren(svg);
          block.dataset.rendered = 'true';
        }).catch(() => {if (block.isConnected) block.textContent = 'Diagram could not be rendered.';});
      }
    }
  }

  class PublicController {
    constructor(root, {window: win = globalThis.window, session = win.lificSession} = {}) {
      this.root = root; this.win = win; this.doc = root.ownerDocument; this.session = session;
      this.project = root.dataset.publicProject; this.kind = root.dataset.topcoatPublic; this.identifier = root.dataset.publicIdentifier || '';
      this.generation = 0; this.disposed = false; this.audience = identity(session); this.projectRow = null; this.issue = null; this.page = null;
      this.comments = []; this.hasOlder = false; this.cursor = null; this.commentAttachments = new Map(); this.attachments = [];
      this.index = null; this.modules = []; this.folders = []; this.focusedIndex = -1;
      this.browse = {query: '', status: this.kind === 'pages' ? 'active' : '', priority: '', module: '', label: '', folder: '', sort: 'priority', direction: 'asc', group: '', density: 'compact', laneBy: 'none', hiddenStatuses: new Set(), collapsedGroups: new Set(), collapsedLanes: new Set(), collapsedColumns: new Set()};
      if (['issues', 'board'].includes(this.kind)) this.loadBrowseState();
      this.previewRows = new Map(); this.previewLoads = new Map(); this.previewAborters = new Set();
      this.client = win.LificTopcoatAttachments?.createClient({session, win}); this.objectUrls = new Set(); this.listeners = [];
      const listen = (target, type, fn) => { target.addEventListener(type, fn); this.listeners.push(() => target.removeEventListener(type, fn)); };
      listen(root, 'click', event => void this.click(event));
      listen(root, 'input', event => this.browseChanged(event));
      listen(root, 'change', event => this.browseChanged(event));
      listen(win, 'keydown', event => this.keydown(event));
      for (const name of ['lific:session-change', 'lific:account-change', 'lific:scope-change']) listen(win, name, () => this.transition());
      listen(win, 'hashchange', () => void this.followDeepLink());
      listen(win, 'popstate', () => void this.followDeepLink());
      listen(win, 'pagehide', event => { if (!event.persisted) this.dispose(); });
      listen(win, 'pageshow', event => { if (event.persisted) this.restore(); });
      void this.load();
    }
    current(generation) { return !this.disposed && generation === this.generation && identity(this.session) === this.project.toUpperCase(); }
    async read(path) {
      if (!this.session || identity(this.session) !== this.project.toUpperCase()) throw new Error('Public project scope changed.');
      const resolved = this.session.resolve(path, 'GET');
      if (!['public', 'synthetic'].includes(resolved?.kind)) throw new Error('Public reads are unavailable for this route.');
      const result = await this.session.request(path, {method: 'GET', credentials: 'omit'});
      if (!result?.ok) { const error = new Error(result?.error || `HTTP ${result?.status || 500}`); error.status = result.status; throw error; }
      return result;
    }
    async data(path) { return (await this.read(path)).data; }
    async load() {
      const generation = ++this.generation;
      this.clear();
      const status = $(this.root, '[data-public-status]'); const output = $(this.root, '[data-public-content]');
      this.root.setAttribute('aria-busy', 'true'); status.textContent = 'Loading…'; output.hidden = true;
      $(this.root, '[data-public-error]').hidden = true;
      try {
        if (!projectName(this.project) || !this.current(generation)) throw new Error('Public project is unavailable.');
        const projects = await this.data('/projects'); if (!this.current(generation)) return;
        this.projectRow = projects.find(row => String(row.identifier || '').toLowerCase() === this.project.toLowerCase());
        if (!this.projectRow) throw new Error('This public project is unavailable.');
        const index = await this.data(`/projects/${this.projectRow.id}/index`); if (!this.current(generation)) return;
        this.index = index;
        if (['issues', 'board', 'pages'].includes(this.kind)) {
          const path = this.kind === 'pages' ? 'folders' : 'modules';
          const references = await this.data(`/${path}?project_id=${this.projectRow.id}`); if (!this.current(generation)) return;
          if (path === 'folders') this.folders = references; else this.modules = references;
          this.renderBrowseControls(output);
        }
        if (this.kind === 'issues' || this.kind === 'board') this.renderIssueIndex(output, index);
        else if (this.kind === 'pages') this.renderPageIndex(output, index);
        else if (this.kind === 'issue-detail') await this.loadIssue(generation, output);
        else if (this.kind === 'page-detail') await this.loadPage(generation, output);
        if (!this.current(generation)) return;
        output.hidden = false; status.textContent = ''; this.root.setAttribute('aria-busy', 'false');
        await this.followDeepLink(generation);
      } catch (error) {
        if (!this.current(generation)) return;
        status.textContent = ''; this.root.setAttribute('aria-busy', 'false');
        const node = $(this.root, '[data-public-error]'); node.hidden = false; node.replaceChildren(text(this.doc, 'p', error.message));
        const retry = text(this.doc, 'button', 'Try again'); retry.type = 'button'; retry.dataset.publicRetry = ''; node.append(retry);
      }
    }
    loadBrowseState() {
      Object.assign(this.browse, {query:'', status:'', priority:'', module:'', label:'', sort:'priority', direction:'asc', group:'', density:'compact', laneBy:'none'});
      const read = (key, fallback) => {try {const raw = this.win.localStorage?.getItem(`${key}${this.project.toUpperCase()}`); if (raw == null) return fallback; try {return JSON.parse(raw);} catch {return typeof fallback === 'string' ? raw : fallback;}} catch {return fallback;}};
      const saved = read('lific:list:state:', {}), b = this.browse;
      if (saved && typeof saved === 'object' && !Array.isArray(saved)) {
        for (const [field, key] of Object.entries({query:'searchQuery', status:'filterStatus', priority:'filterPriority', module:'filterModule', label:'filterLabel'})) if (typeof saved[key] === 'string') b[field] = saved[key];
        if (!['', '@unresolved', ...STATUSES].includes(b.status)) b.status = '';
        if (!['', ...PRIORITIES].includes(b.priority)) b.priority = '';
        if (b.module === '@none') b.module = 'none';
        if (['priority', 'age', 'number', 'updated'].includes(saved.sortField)) b.sort = saved.sortField;
        if (['asc', 'desc'].includes(saved.sortDir)) b.direction = saved.sortDir;
        if (['none', 'status', 'priority', 'module'].includes(saved.groupBy)) b.group = saved.groupBy === 'none' ? '' : saved.groupBy === 'module' ? 'module_id' : saved.groupBy;
        if (['compact', 'comfortable'].includes(saved.density)) b.density = saved.density;
      }
      const lanes = read('lific:board:lanes:', 'none'); if (['none', 'module', 'priority'].includes(lanes)) b.laneBy = lanes;
      for (const [field, key] of Object.entries({hiddenStatuses:'lific:board:hidden-statuses:', collapsedGroups:'lific:list:collapsed:', collapsedLanes:'lific:board:collapsed-lanes:', collapsedColumns:'lific:board:collapsed-columns:'})) {
        const values = read(key, []); b[field] = new Set(Array.isArray(values) ? values.filter(value => typeof value === 'string' && (!['hiddenStatuses', 'collapsedColumns'].includes(field) || STATUSES.includes(value))) : []);
      }
    }
    saveBrowseState() {
      if (!['issues', 'board'].includes(this.kind)) return;
      const b = this.browse, key = this.project.toUpperCase();
      const write = (prefix, value) => {try {this.win.localStorage?.setItem(`${prefix}${key}`, typeof value === 'string' ? value : JSON.stringify(value));} catch { /* Browsing still works when preferences cannot be saved. */ }};
      write('lific:list:state:', {filterStatus:b.status, filterPriority:b.priority, filterLabel:b.label, filterModule:b.module === 'none' ? '@none' : this.modules.find(module => String(module.id) === b.module)?.name || b.module, searchQuery:b.query, sortField:b.sort, sortDir:b.direction, groupBy:b.group === 'module_id' ? 'module' : b.group || 'none', density:b.density});
      write('lific:board:lanes:', b.laneBy);
      for (const [field, prefix] of Object.entries({hiddenStatuses:'lific:board:hidden-statuses:', collapsedGroups:'lific:list:collapsed:', collapsedLanes:'lific:board:collapsed-lanes:', collapsedColumns:'lific:board:collapsed-columns:'})) write(prefix, [...b[field]]);
    }
    browseBuckets(rows, by, includeEmpty = false) {
      const choices = by === 'module' ? [...this.modules.map(module => [String(module.id), module.name]), ['none', 'No module']]
        : (by === 'priority' ? PRIORITIES : STATUSES).map(key => [key, key]);
      if (by === 'module') for (const row of rows) if (row.module_id != null && !choices.some(([key]) => key === String(row.module_id))) choices.push([String(row.module_id), `Module ${row.module_id}`]);
      return choices.map(([key, label]) => ({key, label, rows:rows.filter(row => (by === 'module' ? String(row.module_id ?? 'none') : row[by] || (by === 'status' ? 'backlog' : 'none')) === key)})).filter(bucket => includeEmpty || bucket.rows.length);
    }
    collapseButton(label, field, key) {
      const collapsed = this.browse[field].has(key), button = text(this.doc, 'button', label);
      button.type = 'button'; button.dataset.publicCollapse = field; button.dataset.publicCollapseKey = key;
      button.setAttribute('aria-expanded', String(!collapsed));
      button.setAttribute('aria-label', `${collapsed ? 'Expand' : 'Collapse'} ${label}`); return button;
    }
    renderIssueIndex(output, index) {
      const allRows = Array.isArray(index.issues) ? index.issues : [], rows = this.browseRows(allRows), b = this.browse;
      const results = text(this.doc, 'section'); results.dataset.publicResults = ''; results.dataset.density = b.density; results.setAttribute('aria-label', 'Issues'); output.append(results);
      if (!rows.length) results.append(text(this.doc, 'p', allRows.length ? 'No matching issues.' : 'No public issues.'));
      if (this.kind === 'board') {this.renderIssueBoard(results, rows); return;}
      const list = text(this.doc, 'ul', undefined, 'tc-public__list');
      const groups = b.group && !b.query ? this.browseBuckets(rows, b.group === 'module_id' ? 'module' : b.group) : [{key:'', label:'', rows}];
      for (const group of groups) {
        const collapseKey = `${b.group === 'module_id' ? 'module' : b.group}:${group.key}`;
        if (group.label) {const heading = text(this.doc, 'li', undefined, 'tc-public__group'); heading.dataset.publicGroup = group.label; heading.append(this.collapseButton(`${group.label} group`, 'collapsedGroups', collapseKey), text(this.doc, 'span', ` (${group.rows.length})`)); list.append(heading);}
        if (group.label && b.collapsedGroups.has(collapseKey)) continue;
        for (const raw of group.rows) {
          const issue = publicIssue(raw), li = this.doc.createElement('li'); li.dataset.publicRow = '';
          if (group.label) li.dataset.publicGroup = group.label;
          const link = this.doc.createElement('a'); link.href = routeHref(issueUrl(this.project, issue.identifier), this.win); link.textContent = `${issue.identifier} · ${issue.title}`; link.dataset.publicIssue = issue.identifier;
          li.append(link, text(this.doc, 'span', [issue.status, issue.priority].filter(Boolean).join(' · '), 'tc-public__meta'));
          if (b.density === 'comfortable' && raw.preview) li.append(text(this.doc, 'p', raw.preview, 'tc-public__preview'));
          list.append(li);
        }
      }
      if (rows.length) results.append(list);
    }
    renderIssueBoard(output, rows) {
      const b = this.browse, lanes = b.laneBy === 'none' ? [{key:'', label:'', rows}] : this.browseBuckets(rows, b.laneBy, true);
      const statuses = STATUSES.filter(status => !b.hiddenStatuses.has(status) && (!b.status || (b.status === '@unresolved' ? !['done', 'cancelled'].includes(status) : status === b.status)));
      for (const lane of lanes) {
        const section = text(this.doc, 'section', undefined, 'tc-public__swimlane'); section.dataset.publicSwimlane = lane.key;
        if (lane.label) section.append(this.collapseButton(`${lane.label} lane`, 'collapsedLanes', lane.key), text(this.doc, 'span', ` (${lane.rows.length})`));
        if (lane.label && b.collapsedLanes.has(lane.key)) {output.append(section); continue;}
        const board = text(this.doc, 'div', undefined, 'tc-public__board');
        board.style.gridTemplateColumns = statuses.map(status => b.collapsedColumns.has(status) ? '3rem' : 'minmax(var(--public-column-width,12rem),1fr)').join(' ');
        for (const status of statuses) {
          const column = text(this.doc, 'section', undefined, 'tc-public__lane'), items = lane.rows.filter(row => (row.status || 'backlog') === status), collapsed = b.collapsedColumns.has(status);
          column.dataset.publicLane = status; column.dataset.collapsed = String(collapsed);
          column.append(this.collapseButton(`${status} column`, 'collapsedColumns', status), text(this.doc, 'span', ` (${items.length})`));
          if (!collapsed) for (const raw of items) {
            const issue = publicIssue(raw), card = text(this.doc, 'a', `${issue.identifier} · ${issue.title}`, 'tc-public__card'); card.href = routeHref(issueUrl(this.project, issue.identifier), this.win);
            if (b.density === 'comfortable' && raw.preview) card.append(text(this.doc, 'p', raw.preview, 'tc-public__preview'));
            column.append(card);
          }
          if (!collapsed && !items.length) column.append(text(this.doc, 'p', 'All quiet', 'tc-public__meta'));
          board.append(column);
        }
        section.append(board); output.append(section);
      }
    }
    renderPageIndex(output, index) {
      const allRows = Array.isArray(index.pages) ? index.pages : [];
      const rows = this.browseRows(allRows);
      const results = text(this.doc, 'section'); results.dataset.publicResults = ''; results.setAttribute('aria-label', 'Pages'); output.append(results); output = results;
      if (!rows.length) { output.append(text(this.doc, 'p', allRows.length ? 'No matching pages.' : 'No public pages.')); return; }
      const list = text(this.doc, 'ul', undefined, 'tc-public__list');
      for (const page of rows) { const li = this.doc.createElement('li'); const link = this.doc.createElement('a');
        li.dataset.publicRow = '';
        link.href = routeHref(pageUrl(this.project, page.id), this.win); link.textContent = page.title || 'Untitled page'; li.append(link);
        if (page.pinned) li.append(text(this.doc, 'span', 'Pinned', 'tc-public__meta'));
        if (page.folder_id != null) li.append(text(this.doc, 'span', this.folders.find(folder => folder.id === page.folder_id)?.name || 'Folder', 'tc-public__meta'));
        if (page.status) li.append(text(this.doc, 'span', page.status, 'tc-public__meta'));
        if (page.preview) li.append(text(this.doc, 'p', page.preview, 'tc-public__preview'));
        list.append(li); }
      output.append(list);
    }
    selectControl(parent, name, field, options) {
      const label = text(this.doc, 'label', name); const select = this.doc.createElement('select'); select.dataset.publicBrowse = field;
      select.setAttribute('aria-label', name);
      for (const [value, title] of options) {const option = text(this.doc, 'option', title); option.value = value; select.append(option);}
      if (field === 'module') this.browse.module = String(this.modules.find(module => module.name === this.browse.module)?.id ?? this.browse.module);
      select.value = this.browse[field]; label.append(select); parent.append(label); return select;
    }
    renderBrowseControls(output) {
      const controls = text(this.doc, 'div', undefined, 'tc-public__controls'); controls.dataset.publicControls = '';
      const label = text(this.doc, 'label', this.kind === 'pages' ? 'Search pages' : 'Search issues');
      const search = this.doc.createElement('input'); search.type = 'search'; search.dataset.publicBrowse = 'query'; search.value = this.browse.query; label.append(search); controls.append(label);
      const pages = this.kind === 'pages';
      this.selectControl(controls, 'Status', 'status', pages ? [['active', 'Active'], ['all', 'All'], ['draft', 'Draft'], ['complete', 'Complete'], ['archived', 'Archived']] : [['', 'All'], ['@unresolved', 'Unresolved'], ...STATUSES.map(value => [value, value])]);
      const rows = this.index[pages ? 'pages' : 'issues'] || [];
      this.selectControl(controls, 'Label', 'label', [['', 'All'], ...[...new Set(rows.flatMap(row => row.labels || []))].sort().map(value => [value, value])]);
      if (pages) this.selectControl(controls, 'Folder', 'folder', [['', 'All'], ['root', 'No folder'], ...this.folders.map(folder => [String(folder.id), this.folderLabel(folder)])]);
      else {
        this.selectControl(controls, 'Priority', 'priority', [['', 'All'], ...PRIORITIES.map(value => [value, value])]);
        this.selectControl(controls, 'Module', 'module', [['', 'All'], ['none', 'No module'], ...this.modules.map(module => [String(module.id), module.name])]);
        this.selectControl(controls, 'Sort', 'sort', [['priority', 'Priority'], ['age', 'Created'], ['number', 'Number'], ['updated', 'Updated']]);
        this.selectControl(controls, 'Direction', 'direction', [['asc', 'Ascending'], ['desc', 'Descending']]);
        this.selectControl(controls, 'Density', 'density', [['compact', 'Compact'], ['comfortable', 'Comfortable']]);
        if (this.kind === 'board') this.selectControl(controls, 'Swimlanes', 'laneBy', [['none', 'None'], ['module', 'Module'], ['priority', 'Priority']]);
        if (this.kind === 'issues') this.selectControl(controls, 'Group by', 'group', [['', 'None'], ['status', 'Status'], ['priority', 'Priority'], ['module_id', 'Module']]);
        const layouts = text(this.doc, 'nav'); layouts.setAttribute('aria-label', 'Issue layout');
        for (const [label, path] of [['List', 'issues'], ['Board', 'board']]) {const link = text(this.doc, 'a', label); link.href = routeHref(`/public/${this.project}/${path}`, this.win); if (this.kind === path) link.setAttribute('aria-current', 'page'); layouts.append(link);}
        controls.append(layouts);
      }
      if (this.kind === 'board') for (const status of STATUSES) {const button = text(this.doc, 'button', status); button.type = 'button'; button.dataset.publicColumn = status; button.setAttribute('aria-label', `${this.browse.hiddenStatuses.has(status) ? 'Show' : 'Hide'} ${status} column`); button.setAttribute('aria-pressed', String(!this.browse.hiddenStatuses.has(status))); controls.append(button);}
      output.append(controls);
    }
    folderLabel(folder) {
      const names = [folder.name], seen = new Set([folder.id]); let parent = folder.parent_id;
      while (parent != null && !seen.has(parent)) {seen.add(parent); const row = this.folders.find(item => item.id === parent); if (!row) break; names.unshift(row.name); parent = row.parent_id;}
      return names.join(' / ');
    }
    groupLabel(row) {
      return this.browse.group === 'module_id' ? this.modules.find(module => module.id === row.module_id)?.name || 'No module' : row[this.browse.group] || 'None';
    }
    browseRows(rows) {
      const b = this.browse, pages = this.kind === 'pages';
      const query = b.query.trim(), scores = new Map(rows.map(row => [row.id, query ? Math.max(searchScore(query, row.title), searchScore(query, row.identifier) * .9, searchScore(query, row.preview) * .6, searchScore(query, (row.labels || []).join(' ')) * .55) : 0]));
      const selectedFolder = b.folder && b.folder !== 'root' ? Number(b.folder) : null;
      const inFolder = row => {let folder = row.folder_id; const seen = new Set(); while (folder != null && !seen.has(folder)) {if (folder === selectedFolder) return true; seen.add(folder); folder = this.folders.find(item => item.id === folder)?.parent_id;} return false;};
      return rows.filter(row => (!b.status || b.status === 'all' || (pages && b.status === 'active' ? row.status !== 'archived' : b.status === '@unresolved' ? !['done', 'cancelled'].includes(row.status) : row.status === b.status))
        && (!query || scores.get(row.id) >= .25)
        && (!b.label || (row.labels || []).includes(b.label)) && (!b.priority || row.priority === b.priority)
        && (!b.module || (b.module === 'none' ? row.module_id == null : row.module_id === Number(b.module)))
        && (!b.folder || (b.folder === 'root' ? row.folder_id == null : inFolder(row))))
        .sort((a, c) => {
          if (query) return scores.get(c.id) - scores.get(a.id) || String(a.identifier || a.title).localeCompare(c.identifier || c.title);
          if (pages) return Number(Boolean(c.pinned)) - Number(Boolean(a.pinned)) || String(c.updated_at || '').localeCompare(a.updated_at || '') || a.title.localeCompare(c.title);
          const group = b.group && !b.query ? this.groupLabel(a).localeCompare(this.groupLabel(c)) : 0;
          let order = b.sort === 'priority' ? PRIORITIES.indexOf(a.priority) - PRIORITIES.indexOf(c.priority) : b.sort === 'number' ? Number(a.identifier?.split('-').pop()) - Number(c.identifier?.split('-').pop()) : String(a[b.sort === 'age' ? 'created_at' : 'updated_at'] || '').localeCompare(c[b.sort === 'age' ? 'created_at' : 'updated_at'] || '');
          if (b.sort === 'priority' && !order) order = String(c.created_at || '').localeCompare(a.created_at || '');
          return group || (b.direction === 'desc' ? -order : order) || String(a.identifier).localeCompare(c.identifier);
        }).slice(0, query ? 50 : rows.length);
    }
    browseChanged(event) {
      const field = event.target.dataset.publicBrowse; if (!field || !(field in this.browse) || this.browse[field] === event.target.value) return;
      this.browse[field] = event.target.value; this.saveBrowseState(); this.focusedIndex = -1; this.renderBrowseResults();
    }
    renderBrowseResults() {
      const output = $(this.root, '[data-public-content]'); $(output, '[data-public-results]')?.remove();
      if (this.kind === 'pages') this.renderPageIndex(output, this.index); else this.renderIssueIndex(output, this.index);
    }
    keydown(event) {
      if (this.current(this.generation) && event.target.matches?.('[data-public-lightbox]') && ['Enter', ' '].includes(event.key)) {event.preventDefault(); void this.openLightbox(event.target); return;}
      if (this.disposed || !this.current(this.generation) || !['issues', 'board', 'pages'].includes(this.kind) || event.ctrlKey || event.metaKey || event.altKey || event.target.closest?.('input,textarea,select,[contenteditable],dialog')) return;
      if (event.key === '/') {event.preventDefault(); $(this.root, '[data-public-browse="query"]')?.focus(); return;}
      if (this.kind !== 'issues') return;
      const links = [...this.root.querySelectorAll('[data-public-issue]')]; if (!links.length) return;
      const active = links.indexOf(this.doc.activeElement); if (active >= 0) this.focusedIndex = active;
      if (['j', 'ArrowDown', 'k', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        event.preventDefault(); this.focusedIndex = event.key === 'Home' ? 0 : event.key === 'End' ? links.length - 1 : Math.max(0, Math.min(links.length - 1, this.focusedIndex + (['j', 'ArrowDown'].includes(event.key) ? 1 : -1)));
        links[this.focusedIndex].focus(); links[this.focusedIndex].scrollIntoView?.({block: 'nearest'});
      } else if (event.key === 'Escape') {this.focusedIndex = -1; this.doc.activeElement?.blur();}
    }
    async loadIssue(generation, output) {
      const resolved = await this.data(`/issues/resolve/${encodeURIComponent(this.identifier)}`);
      if (!this.current(generation)) return;
      const id = typeof resolved === 'number' ? resolved : resolved.id;
      if (!Number.isSafeInteger(id) || id < 0) throw new Error('This public issue is unavailable.');
      const issue = await this.data(`/issues/${id}`); if (!this.current(generation)) return;
      if (issue.project_id != null && issue.project_id !== this.projectRow.id) throw new Error('This public issue is unavailable.');
      this.issue = issue;
      const title = text(this.doc, 'h2', issue.title || 'Untitled issue'); title.id = `issue-${id}`; output.append(title);
      output.append(text(this.doc, 'p', [issue.identifier || this.identifier, issue.status, issue.priority].filter(Boolean).join(' · '), 'tc-public__meta'));
      const body = this.doc.createElement('article'); body.className = 'tc-public__markdown'; renderMarkdown(this.doc, body, issue.description || issue.body || '', this.project); output.append(body);
      this.hydrateImages(body, generation);
      const attach = await this.loadAttachments('issue', id, generation); if (!this.current(generation)) return;
      this.renderAttachments(output, attach, 'issue');
      await this.loadComments('issue', id, generation); if (!this.current(generation)) return;
      await this.loadCommentAttachments(generation); if (!this.current(generation)) return;
      this.renderComments(output);
    }
    async loadPage(generation, output) {
      const id = Number(this.identifier); if (!Number.isSafeInteger(id) || id < 0) throw new Error('This public page is unavailable.');
      const page = await this.data(`/pages/${id}`); if (!this.current(generation)) return;
      if (page.project_id != null && page.project_id !== this.projectRow.id) throw new Error('This public page is unavailable.');
      this.page = page;
      const title = text(this.doc, 'h2', page.title || 'Untitled page'); title.id = `page-${id}`; output.append(title);
      if (page.status) output.append(text(this.doc, 'p', page.status, 'tc-public__meta'));
      const body = this.doc.createElement('article'); body.className = 'tc-public__markdown'; renderMarkdown(this.doc, body, page.content || page.body || '', this.project); output.append(body);
      this.hydrateImages(body, generation);
      const attach = await this.loadAttachments('page', id, generation); if (!this.current(generation)) return;
      this.renderAttachments(output, attach, 'page');
      await this.loadComments('page', id, generation); if (!this.current(generation)) return;
      await this.loadCommentAttachments(generation); if (!this.current(generation)) return;
      this.renderComments(output);
    }
    async loadAttachments(entityType, id, generation) {
      if (!this.client) return [];
      const result = await this.client.list({entity_type: entityType, entity_id: id});
      if (!this.current(generation)) return [];
      if (!result.ok) throw new Error(result.error || 'Could not load public attachments.');
      this.attachments = result.data || []; return this.attachments;
    }
    async loadComments(entityType, id, generation, cursor = null, existing = []) {
      const params = new URLSearchParams({order: 'desc', limit: '51'});
      if (cursor) { params.set('before_created_at', cursor.created_at); params.set('before_id', String(cursor.id)); }
      const result = await this.read(`/${entityType === 'issue' ? 'issues' : 'pages'}/${id}/comments?${params}`);
      if (!this.current(generation)) return;
      const descending = Array.isArray(result.data) ? result.data : result.data?.items || [];
      this.hasOlder = descending.length > 50 || result.headers?.get?.('x-comment-has-more') === 'true';
      const page = (descending.length > 50 ? descending.slice(0, 50) : descending).reverse();
      this.comments = [...page, ...existing]; this.cursor = commentCursor(page) || cursor;
    }
    async loadCommentAttachments(generation) {
      if (!this.client) return;
      const next = new Map();
      for (const comment of this.comments) {
        const result = await this.client.list({entity_type: 'comment', entity_id: comment.id});
        if (!this.current(generation)) return;
        if (result.ok) next.set(comment.id, result.data || []);
      }
      this.commentAttachments = next;
    }
    renderComments(output) {
      const section = text(this.doc, 'section', undefined, 'tc-public__comments'); section.dataset.publicComments = '';
      section.append(text(this.doc, 'h3', 'Comments'));
      const list = this.doc.createElement('ol');
      for (const comment of this.comments) {
        const row = this.doc.createElement('li'); row.id = `comment-${comment.id}`;
        const author = comment.author_display_name || comment.author || 'Public contributor';
        row.append(text(this.doc, 'p', author, 'tc-public__meta'));
        const body = this.doc.createElement('div'); body.className = 'tc-public__markdown'; renderMarkdown(this.doc, body, comment.content || '', this.project); row.append(body);
        for (const attachment of this.commentAttachments.get(comment.id) || []) row.append(this.attachmentLink(attachment, `comment-attachment-${attachment.id}`));
        list.append(row);
      }
      if (!this.comments.length) section.append(text(this.doc, 'p', 'No public comments.'));
      else section.append(list);
      if (this.hasOlder) { const more = this.doc.createElement('a'); more.href = '#older-comments'; more.dataset.publicMoreComments = ''; more.textContent = 'Load older comments'; section.append(more); }
      output.append(section);
      void this.hydrateImages(section, this.generation);
    }
    renderAttachments(output, attachments, entity) {
      if (!attachments.length) return;
      const section = text(this.doc, 'section', undefined, 'tc-public__attachments'); section.append(text(this.doc, 'h3', 'Attachments'));
      const list = this.doc.createElement('ul');
      for (const attachment of attachments) list.append(this.attachmentLink(attachment, `attachment-${attachment.id}`));
      section.append(list); output.append(section);
    }
    attachmentLink(attachment, id) {
      const row = this.doc.createElement('li'); row.id = id; row.dataset.publicAttachment = String(attachment.id);
      const link = this.doc.createElement('a'); link.href = '#download'; link.dataset.publicDownload = String(attachment.id);
      link.textContent = `${attachment.filename || 'Attachment'}${Number.isFinite(attachment.size_bytes) ? ` · ${formatBytes(attachment.size_bytes)}` : ''}`;
      row.append(link);
      const kind = this.win.LificTopcoatAttachments?.viewerKind(attachment) || 'file';
      if (kind !== 'file') {
        const preview = text(this.doc, 'button', 'Preview'); preview.type = 'button'; preview.dataset.publicPreview = String(attachment.id); row.append(preview);
        const content = text(this.doc, 'div'); content.dataset.publicPreviewContent = ''; content.hidden = true; row.append(content);
      }
      this.previewRows.set(row, attachment); return row;
    }
    async attachmentBlob(attachment, generation) {
      const chunks = []; let size = 0; const aborter = new AbortController(); this.previewAborters.add(aborter);
      try {
        const result = await this.client.streamDownload(attachment.id, {signal: aborter.signal, open: () => ({
          write: chunk => {size += chunk.byteLength; if (size > this.win.LificTopcoatAttachments.MAX_INLINE_BYTES) throw new Error('File is too large to preview inline. Use Download original.'); chunks.push(chunk);}, close() {}, abort() {chunks.length = 0;},
        })});
        if (!this.current(generation)) return null;
        if (!result.ok) throw new Error(result.error || 'Preview unavailable. Use Download original.');
        const blob = new Blob(chunks, {type: result.contentType || attachment.mime});
        const url = this.win.URL.createObjectURL(blob); this.objectUrls.add(url); return url;
      } finally {this.previewAborters.delete(aborter);}
    }
    async attachmentMediaUrl(attachment, generation) {
      if (!this.current(generation) || this.session.resolve(`/attachments/${attachment.id}`, 'GET').kind !== 'public') throw new Error('Public media unavailable.');
      const workers = this.win.navigator?.serviceWorker;
      if (!this.win.isSecureContext || !workers) return this.attachmentBlob(attachment, generation);
      try {
        const worker = routeHref('/__topcoat-public-media.js', this.win), scope = routeHref('/public/', this.win);
        const registration = await workers.register(worker, {scope, updateViaCache:'none'});
        const expected = new URL(worker, this.win.location.href).href;
        if (workers.controller?.scriptURL !== expected) await new Promise((resolve, reject) => {
          const timer = this.win.setTimeout(() => {workers.removeEventListener('controllerchange', changed); reject(new Error('Public media transport unavailable.'));}, 10000);
          const changed = () => {if (workers.controller?.scriptURL === expected) {this.win.clearTimeout(timer); workers.removeEventListener('controllerchange', changed); resolve();}};
          workers.addEventListener('controllerchange', changed); changed();
        });
        if (!this.current(generation) || registration.scope !== new URL(scope, this.win.location.href).href) return null;
        return routeHref(`/public/${encodeURIComponent(this.project)}/_media/${attachment.id}`, this.win);
      } catch (error) {
        if (!this.current(generation)) return null;
        // Older browsers retain the bounded local-Blob preview and download
        // fallback. Large supported media always uses lazy HTTP ranges.
        if (attachment.size_bytes > this.win.LificTopcoatAttachments.MAX_INLINE_BYTES) throw error;
        return this.attachmentBlob(attachment, generation);
      }
    }
    async preview(row, target = null) {
      const attachment = this.previewRows.get(row); if (!attachment || !this.client) return;
      const output = $(row, '[data-public-preview-content]'); if (!output) return;
      const generation = this.generation;
      if (this.previewLoads.has(row)) {await this.previewLoads.get(row); if (this.current(generation) && target) this.selectLines(output, target); return;}
      const button = $(row, '[data-public-preview]'); button.disabled = true; output.hidden = false; output.textContent = 'Loading preview…'; delete output.dataset.publicPreviewError;
      const task = (async () => {
        try {
          const kind = this.win.LificTopcoatAttachments.viewerKind(attachment);
          if (['image', 'video', 'audio'].includes(kind)) {
            if (attachment.mime === 'image/svg+xml') throw new Error('Preview unavailable for this file. Use Download original.');
            const url = await (kind === 'image' ? this.attachmentBlob(attachment, generation) : this.attachmentMediaUrl(attachment, generation)); if (!this.current(generation) || !url) return;
            const media = this.doc.createElement(kind === 'image' ? 'img' : kind); media.src = url;
            if (kind === 'image') {media.alt = attachment.alt_text || attachment.filename; media.dataset.publicLightbox = ''; media.tabIndex = 0; media.setAttribute('role', 'button'); media.setAttribute('aria-label', `Open image ${media.alt}`);}
            else {
              media.controls = true; media.preload = 'metadata';
              const stop = () => {media.pause(); media.removeAttribute('src'); media.load();};
              const changed = () => {if (!this.current(generation)) stop();};
              const hidden = event => {if (!event.persisted) stop();};
              const names = ['lific:session-change','lific:account-change','lific:scope-change'];
              for (const name of names) this.win.addEventListener(name, changed);
              this.win.addEventListener('pagehide', hidden);
              this.listeners.push(() => {stop(); for (const name of names) this.win.removeEventListener(name, changed); this.win.removeEventListener('pagehide', hidden);});
            }
            media.addEventListener('error', () => {if (this.current(generation)) output.textContent = 'Preview unavailable in this browser. Use Download original.';}, {once:true});
            output.replaceChildren(media);
          } else if (['text', 'diff', 'csv', 'json'].includes(kind)) {
            const aborter = new AbortController(); this.previewAborters.add(aborter);
            let result; try {result = await this.client.text(attachment.id, {signal:aborter.signal});} finally {this.previewAborters.delete(aborter);}
            if (!this.current(generation)) return; if (!result.ok) throw new Error(result.error || 'Preview unavailable.');
            output.replaceChildren();
            if (kind === 'csv' && this.win.LificTopcoatFiles) this.renderData(output, result.text, attachment.filename);
            else if (kind === 'json') {try {const pre = text(this.doc, 'pre', JSON.stringify(JSON.parse(result.text), null, 2)); output.append(pre);} catch {output.append(text(this.doc, 'p', 'Invalid JSON. Showing original text.')); this.renderText(output, result.text, attachment.id);}}
            else this.renderText(output, result.text, attachment.id, kind === 'diff');
          } else {
            const aborter = new AbortController(); this.previewAborters.add(aborter);
            let result; try {result = await this.client.preview(attachment.id, {signal:aborter.signal});} finally {this.previewAborters.delete(aborter);}
            if (!this.current(generation)) return;
            if (!result.ok) throw new Error(result.error || 'Preview unavailable.');
            output.replaceChildren(); const list = this.doc.createElement('ul');
            if (result.data?.kind === 'zip') {for (const entry of result.data.entries || []) list.append(text(this.doc, 'li', `${entry.name} · ${formatBytes(entry.size)}`)); if (result.data.truncated) output.append(text(this.doc, 'p', `Archive preview truncated (${result.data.total_entries} entries).`));}
            else if (result.data?.kind === 'sqlite') {for (const table of result.data.tables || []) list.append(text(this.doc, 'li', `${table.name} · ${table.rows} rows`));}
            else throw new Error('Preview unavailable for this file. Use Download original.');
            output.append(list);
          }
          if (target) this.selectLines(output, target);
        } catch (error) {if (this.current(generation)) {output.textContent = error.message.includes('Download original') ? error.message : `${error.message} Use Download original.`; output.dataset.publicPreviewError = 'true';}}
        finally {if (this.current(generation)) button.disabled = false;}
      })();
      this.previewLoads.set(row, task); await task; if (output.dataset.publicPreviewError === 'true') this.previewLoads.delete(row);
    }
    renderText(output, source, attachmentId, diff = false) {
      const findLabel = text(this.doc, 'label', 'Find in file'); const find = this.doc.createElement('input'); find.type = 'search'; findLabel.append(find); output.append(findLabel);
      const code = text(this.doc, 'pre', undefined, 'tc-public__file');
      for (const [index, line] of source.replace(/\r\n?/g, '\n').split('\n').entries()) {
        const row = text(this.doc, 'span'); row.dataset.publicLine = String(index + 1); row.id = `att${attachmentId}-L${index + 1}`;
        const link = text(this.doc, 'a', index + 1); link.href = `#att${attachmentId}-L${index + 1}`; link.setAttribute('aria-label', `Select line ${index + 1}`);
        const body = text(this.doc, 'code', line); row.append(link, body); if (diff && /^[+-]/.test(line)) row.classList.add(line[0] === '+' ? 'tc-public__addition' : 'tc-public__deletion'); code.append(row);
      }
      find.addEventListener('input', () => {const query = find.value.toLowerCase(); for (const row of code.children) {row.classList.toggle('tc-public__match', Boolean(query) && row.querySelector('code').textContent.toLowerCase().includes(query));} code.querySelector('.tc-public__match')?.scrollIntoView?.({block:'nearest'});});
      output.append(code);
    }
    selectLines(output, target) {
      for (const row of output.querySelectorAll('[data-public-line]')) {const number = Number(row.dataset.publicLine); row.classList.toggle('tc-public__target', number >= target.start && number <= target.end);}
      $(output, `[data-public-line="${target.start}"]`)?.scrollIntoView?.({block:'center'});
    }
    renderData(output, source, filename) {
      const parser = this.win.LificTopcoatFiles; const data = parser.parseDelimited(source, {delimiter:parser.detectDelimiter(filename, source)});
      const table = this.doc.createElement('table'); const head = this.doc.createElement('thead'), header = this.doc.createElement('tr'), body = this.doc.createElement('tbody');
      table.append(text(this.doc, 'caption', `${data.totalRows} rows${data.truncated ? ' (preview truncated)' : ''}`));
      let direction = 'asc', column = null;
      const render = () => {body.replaceChildren(); for (const values of column === null ? data.rows : parser.sortRows(data.rows, column, direction)) {const row = this.doc.createElement('tr'); for (const value of values) row.append(text(this.doc, 'td', value)); body.append(row);}};
      data.headers.forEach((value, index) => {const th = this.doc.createElement('th'); th.scope = 'col'; const button = text(this.doc, 'button', value || `Column ${index + 1}`); button.type = 'button'; button.addEventListener('click', () => {direction = column === index && direction === 'asc' ? 'desc' : 'asc'; column = index; for (const cell of header.children) cell.removeAttribute('aria-sort'); th.setAttribute('aria-sort', direction === 'asc' ? 'ascending' : 'descending'); render();}); th.append(button); header.append(th);});
      head.append(header); table.append(head, body); output.append(table); render();
    }
    async openLightbox(image) {
      const generation = this.generation;
      let src = image.src;
      if (image.dataset.publicImage) {
        try {src = await this.attachmentBlob({id:Number(image.dataset.publicImage), filename:image.alt}, generation);}
        catch { /* A bounded original failure still permits the safe thumbnail. */ }
        if (!this.current(generation)) return;
      }
      const dialog = this.doc.createElement('dialog'); dialog.className = 'tc-public__lightbox'; dialog.setAttribute('aria-label', 'Image preview');
      const full = this.doc.createElement('img'); full.src = src; full.alt = image.alt;
      const close = text(this.doc, 'button', 'Close image'); close.type = 'button'; close.addEventListener('click', () => dialog.close());
      dialog.append(close, full); this.root.append(dialog); dialog.addEventListener('close', () => {dialog.remove(); image.focus();}, {once:true}); dialog.showModal();
    }
    async hydrateImages(target, generation) {
      if (!this.client) return;
      for (const image of target.querySelectorAll('[data-public-image]')) {
        const id = Number(image.dataset.publicImage); const result = await this.client.thumbnail(id);
        if (!this.current(generation)) return;
        if (result.ok && result.blob) { const url = this.win.URL.createObjectURL(result.blob); this.objectUrls.add(url); image.src = url; image.dataset.publicLightbox = ''; image.tabIndex = 0; image.setAttribute('role', 'button'); image.setAttribute('aria-label', `Open image ${image.alt || id}`); }
        else { const fallback = text(this.doc, 'a', `Download image ${image.alt || id}`); fallback.href = '#download'; fallback.dataset.publicDownload = String(id); image.replaceWith(fallback); }
      }
    }
    async olderComments() {
      const generation = this.generation;
      const type = this.kind === 'issue-detail' ? 'issue' : 'page';
      const id = type === 'issue' ? this.issue?.id : this.page?.id;
      if (!id || !this.cursor || !this.hasOlder) return false;
      const existing = this.comments;
      const previousCursor = this.cursor;
      try {
        await this.loadComments(type, id, generation, this.cursor, existing);
        if (!this.current(generation)) return false;
        this.commentAttachments.clear(); await this.loadCommentAttachments(generation);
        if (!this.current(generation)) return false;
        $(this.root, '[data-public-content]').querySelector('[data-public-comments]')?.remove();
        this.renderComments($(this.root, '[data-public-content]'));
        return this.comments.length > existing.length || this.cursor?.id !== previousCursor?.id || this.cursor?.created_at !== previousCursor?.created_at;
      } catch (error) {
        const status = text(this.doc, 'p', `Could not load older comments: ${error.message}`, 'tc-public__error');
        $(this.root, '[data-public-comments]')?.append(status);
        return false;
      }
    }
    async download(id, anchor) {
      if (!this.client) return;
      const generation = this.generation;
      anchor.setAttribute('aria-busy', 'true');
      const chunks = [];
      const result = await this.client.streamDownload(id, {filename: anchor.textContent, open: () => ({write: chunk => chunks.push(chunk), close() {}, abort() {chunks.length = 0;}})});
      if (!this.current(generation)) return;
      anchor.removeAttribute('aria-busy');
      if (!result.ok) { anchor.textContent = `${anchor.textContent} · ${result.error}`; return; }
      const blob = new Blob(chunks, {type: result.contentType || 'application/octet-stream'});
      const url = this.win.URL.createObjectURL(blob); this.objectUrls.add(url);
      const link = this.doc.createElement('a'); link.href = url; link.download = result.filename || anchor.textContent; link.hidden = true;
      this.doc.body.append(link); link.click(); link.remove();
    }
    async click(event) {
      const retry = event.target.closest('[data-public-retry]'); if (retry && this.root.contains(retry)) {await this.load(); return;}
      const collapse = event.target.closest('[data-public-collapse]');
      if (collapse && this.root.contains(collapse)) {const set = this.browse[collapse.dataset.publicCollapse], key = collapse.dataset.publicCollapseKey; set.has(key) ? set.delete(key) : set.add(key); this.saveBrowseState(); this.renderBrowseResults(); const replacement = [...this.root.querySelectorAll('[data-public-collapse]')].find(node => node.dataset.publicCollapse === collapse.dataset.publicCollapse && node.dataset.publicCollapseKey === key); replacement?.focus(); return;}
      const column = event.target.closest('[data-public-column]'); if (column && this.root.contains(column)) {const status = column.dataset.publicColumn; this.browse.hiddenStatuses.has(status) ? this.browse.hiddenStatuses.delete(status) : this.browse.hiddenStatuses.add(status); column.setAttribute('aria-pressed', String(!this.browse.hiddenStatuses.has(status))); column.setAttribute('aria-label', `${this.browse.hiddenStatuses.has(status) ? 'Show' : 'Hide'} ${status} column`); this.saveBrowseState(); this.renderBrowseResults(); return;}
      const preview = event.target.closest('[data-public-preview]'); if (preview && this.root.contains(preview)) {await this.preview(preview.closest('[data-public-attachment]')); return;}
      const image = event.target.closest('[data-public-lightbox]'); if (image && this.root.contains(image)) {await this.openLightbox(image); return;}
      const more = event.target.closest('[data-public-more-comments]');
      if (more && this.root.contains(more)) { event.preventDefault(); await this.olderComments(); return; }
      const download = event.target.closest('[data-public-download]');
      if (download && this.root.contains(download)) { event.preventDefault(); await this.download(Number(download.dataset.publicDownload), download); }
    }
    async followDeepLink(generation = this.generation) {
      const query = new URLSearchParams(this.win.location.search || '');
      const fragment = /^#(?:comment-[1-9]\d*|(?:attachment-|att)[1-9]\d*(?:-L[1-9]\d*(?:-[1-9]\d*)?)?)$/.test(this.win.location.hash || '') ? this.win.location.hash : '';
      const comment = fragment ? fragment.match(/^#comment-([1-9]\d*)$/)?.[1] : query.get('comment');
      const reference = fragment || query.get('att') || '';
      const lineTarget = attachmentTarget(reference);
      const attachment = lineTarget?.id || reference.match(/^#?(?:att|attachment-)?([1-9]\d*)(?:-L[1-9]\d*(?:-[1-9]\d*)?)?$/)?.[1];
      const targetVisible = () => (!comment || this.doc.getElementById(`comment-${comment}`))
        && (!attachment || this.doc.getElementById(`attachment-${attachment}`) || this.doc.getElementById(`comment-attachment-${attachment}`));
      while ((comment || attachment) && !targetVisible() && this.hasOlder && this.current(generation)) {
        if (!await this.olderComments()) break;
      }
      if (!this.current(generation)) return;
      if (comment) { const row = this.doc.getElementById(`comment-${comment}`); if (row) { row.id = `comment-${comment}`; row.scrollIntoView?.({block: 'center'}); } }
      if (attachment) { const row = this.doc.getElementById(`attachment-${attachment}`) || this.doc.getElementById(`comment-attachment-${attachment}`); row?.scrollIntoView?.({block: 'center'}); row?.classList.add('tc-public__target'); if (row && lineTarget) await this.preview(row, lineTarget); }
    }
    transition() {
      const next = identity(this.session); if (next === this.audience) return;
      this.audience = next; this.generation++; this.clear();
      const project = this.root.dataset.publicProject;
      if (project.toUpperCase() !== this.project.toUpperCase()) {this.project = project; if (['issues', 'board'].includes(this.kind)) this.loadBrowseState();}
      void this.load();
    }
    restore() {
      this.disposed = false; this.audience = identity(this.session); this.generation++; this.clear(); void this.load();
    }
    clear() {
      for (const aborter of this.previewAborters) aborter.abort(); this.previewAborters.clear(); this.previewRows.clear(); this.previewLoads.clear();
      for (const dialog of this.root.querySelectorAll('.tc-public__lightbox')) {dialog.close(); dialog.remove();}
      this.comments = []; this.attachments = []; this.commentAttachments.clear();
      this.index = null; this.modules = []; this.folders = []; this.issue = null; this.page = null; this.projectRow = null;
      $(this.root, '[data-public-content]').replaceChildren();
      for (const url of this.objectUrls) this.win.URL.revokeObjectURL(url); this.objectUrls.clear();
    }
    dispose() { this.disposed = true; this.generation++; for (const remove of this.listeners) remove(); this.listeners = []; this.clear(); }
  }
  function formatBytes(size) { if (!Number.isFinite(size) || size < 0) return ''; if (size < 1024) return `${size} B`; const units=['KB','MB','GB']; let value=size/1024, unit=0; while(value>=1024&&unit<units.length-1){value/=1024;unit++;} return `${value.toFixed(value>=10?0:1)} ${units[unit]}`; }
  function attach(root, options) { const controller = new PublicController(root, options); return {controller, dispose: () => controller.dispose()}; }
  const api = Object.freeze({PublicController, attach, renderMarkdown, formatBytes, publicHref, attachmentTarget, searchScore});
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  if (typeof globalThis !== 'undefined') globalThis.LificTopcoatPublic = api;
  if (typeof window !== 'undefined') {
    const mount = () => { for (const root of document.querySelectorAll('[data-topcoat-public]:not([data-mounted])')) { root.dataset.mounted = 'true'; attach(root); } };
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount, {once: true}); else mount();
  }
})();
