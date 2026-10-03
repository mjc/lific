(() => {
'use strict';

const STATUS_RANK = {active: 0, todo: 1};
const PRIORITY_RANK = {urgent: 0, high: 1, medium: 2, low: 3, none: 4};
const TRANSIENT_ROLE = {role: null, enforced: false, is_admin: false};

function compareIssues(a, b) {
  return (STATUS_RANK[a.status] ?? 9) - (STATUS_RANK[b.status] ?? 9)
    || (PRIORITY_RANK[a.priority] ?? 9) - (PRIORITY_RANK[b.priority] ?? 9)
    || b.updated_at.localeCompare(a.updated_at);
}

function canEdit(role) {
  return !!role && (!role.enforced || role.is_admin || ['maintainer', 'lead'].includes(role.role));
}

function canManage(role) {
  return !!role && (!role.enforced || role.is_admin || role.role === 'lead');
}

function topProjectIds(issues, projects) {
  const visible = new Set(projects.map(project => project.id));
  const lastSeen = new Map();
  for (const issue of issues) {
    if (visible.has(issue.project_id) && (!lastSeen.has(issue.project_id) || issue.updated_at > lastSeen.get(issue.project_id))) {
      lastSeen.set(issue.project_id, issue.updated_at);
    }
  }
  return lastSeen.size
    ? [...lastSeen].sort((a, b) => b[1].localeCompare(a[1])).slice(0, 3).map(([id]) => id)
    : [...projects].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 3).map(project => project.id);
}

function projectRoute(project, section, id = null) {
  return `/${encodeURIComponent(project.identifier)}/${section}${id === null ? '' : `/${encodeURIComponent(id)}`}`;
}

function recentItems(entries, projects) {
  return entries.flatMap(entry => {
    const project = projects.find(project => project.identifier === entry.project);
    const section = {issue: 'issues', page: 'pages', plan: 'plans'}[entry.type];
    const valid = entry.type === 'issue'
      ? /^[A-Za-z][A-Za-z0-9_-]*-\d+$/.test(entry.routeId)
      : /^\d+$/.test(entry.routeId);
    return project && section && valid
      ? [{...entry, href: projectRoute(project, section, entry.routeId)}] : [];
  }).slice(0, 8);
}

function activityHref(activity, projects) {
  const project = projects.find(project => project.id === activity.project_id);
  if (!project) return null;
  switch (activity.entity_type) {
    case 'issue': return activity.entity_label ? projectRoute(project, 'issues', activity.entity_label) : null;
    case 'page': return projectRoute(project, 'pages', activity.entity_id);
    case 'comment':
      if (activity.issue_id != null && activity.entity_label) return projectRoute(project, 'issues', activity.entity_label);
      return activity.page_id != null ? projectRoute(project, 'pages', activity.page_id) : null;
    default: return null;
  }
}

function homeModel({projects, issues, pages = [], groups = [], activity = [], recents = [], quickProjectId = null, role = null}) {
  const issueGroups = projects.map(project => {
    const rows = issues.filter(issue => issue.project_id === project.id && issue.status in STATUS_RANK).sort(compareIssues);
    return {project, visible: rows.slice(0, 6), total: rows.length};
  }).filter(group => group.total > 0).sort((a, b) => b.total - a.total || a.project.name.localeCompare(b.project.name));
  const assigned = new Set();
  const projectGroups = [...groups].sort((a, b) => a.sort_order - b.sort_order || a.id - b.id).map(group => {
    const members = projects.filter(project => group.project_ids.includes(project.id) && !assigned.has(project.id));
    members.forEach(project => assigned.add(project.id));
    return {...group, projects: members};
  });
  const ungrouped = projects.filter(project => !assigned.has(project.id));
  if (ungrouped.length) projectGroups.push({id: null, name: 'Projects', projects: ungrouped});
  const pinnedPages = pages.filter(page => page.pinned && projects.some(project => project.id === page.project_id))
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 8)
    .map(page => ({...page, href: projectRoute(projects.find(project => project.id === page.project_id), 'pages', page.id)}));
  const quickProject = projects.find(project => project.id === quickProjectId);
  return {projects: [...projects], issueGroups, projectGroups, pinnedPages,
    issueTotal: issueGroups.reduce((total, group) => total + group.total, 0),
    recents: recentItems(recents, projects),
    activity: [...activity].sort((a, b) => b.ts.localeCompare(a.ts)).slice(0, 10)
      .map(row => ({...row, href: activityHref(row, projects)})),
    newIssueHref: quickProject && canEdit(role) ? projectRoute(quickProject, 'issues', 'new') : null};
}

function daysSince(value, now) {
  const timestamp = Date.parse(/[zZ]|[+-]\d{2}:?\d{2}$/.test(value) ? value : `${value}Z`);
  return Number.isFinite(timestamp) ? Math.max(0, Math.floor((now - timestamp) / 86400000)) : 0;
}

function overviewModel({project, issues, counts, activity, role}, now = Date.now()) {
  const weights = {urgent: 100, high: 55, medium: 25, low: 10, none: 4};
  const multiplier = {todo: 1.25, active: 1.15, backlog: 1};
  const score = issue => ((weights[issue.priority] ?? 4) + daysSince(issue.created_at, now) * 0.5
    + daysSince(issue.updated_at, now) * 0.6) * multiplier[issue.status];
  const open = issues.filter(issue => issue.status in multiplier).sort((a, b) => score(b) - score(a));
  return {project, counts, completion: counts?.total ? Math.round(counts.done / counts.total * 100) : 0,
    attention: open.slice(0, 6).map(issue => {
      const age = daysSince(issue.created_at, now);
      const idle = daysSince(issue.updated_at, now);
      const label = days => days >= 60 ? `${Math.round(days / 30)}mo` : days >= 1 ? `${days}d` : 'today';
      return {...issue, ageLabel: label(age), idleLabel: idle >= 14 ? label(idle) : null,
        heat: issue.priority === 'urgent' ? 'urgent' : issue.priority === 'high' || idle >= 14 ? 'warning' : 'quiet'};
    }), moreCount: Math.max(0, open.length - 6),
    activity: activity.map(row => ({...row, href: activityHref(row, [project])})),
    newIssueHref: canEdit(role) ? projectRoute(project, 'issues', 'new') : null,
    settingsHref: canManage(role) ? projectRoute(project, 'settings') : null};
}

function createActivityCounter() {
  let baseline = 0;
  let baselineAt = null;
  let events = [];
  return {
    seed(value, now) { baseline = value; baselineAt = now; events = []; },
    record(now) { events.push(now); },
    reset() { baseline = 0; baselineAt = null; events = []; },
    rate(now) {
      events = events.filter(at => at >= now - 86400000);
      for (const [duration, unit] of [[1000, 'updates/s'], [60000, 'updates/min'], [3600000, 'updates/hr']]) {
        const value = events.filter(at => at >= now - duration).length;
        if (value >= 2) return {value, unit};
      }
      return {value: events.length + (baselineAt !== null && now - baselineAt < 86400000 ? baseline : 0), unit: 'updates/day'};
    },
  };
}

class DashboardController {
  constructor(env, identifier = null) {
    this.env = env;
    this.identifier = identifier;
    this.state = {status: 'idle', model: null, error: '', sectionErrors: {}, refreshing: false};
    this.generation = 0;
    this.abort = null;
    this.timer = null;
    this.firstInvalidation = null;
    this.disposed = false;
    this.counter = createActivityCounter();
    this.activityReady = false;
  }

  publish(state) {
    this.state = state;
    this.env.onChange?.(state);
  }

  activityRate() { return this.counter.rate(this.env.now()); }

  async load(initial = this.state.model === null) {
    const identity = this.env.identity();
    const generation = ++this.generation;
    this.abort?.abort();
    this.env.cancel(this.timer);
    this.timer = null;
    this.firstInvalidation = null;
    if (this.disposed || identity === null) {
      if (!this.disposed) this.publish({status: 'idle', model: null, error: '', sectionErrors: {}, refreshing: false});
      return;
    }
    const abort = new AbortController();
    this.abort = abort;
    const current = () => !this.disposed && generation === this.generation && this.env.identity() === identity;
    const sectionErrors = {};
    const request = async path => {
      try { return await this.env.request(path, {signal: abort.signal}); }
      catch (error) { return {ok: false, status: 0, error: error.message}; }
    };
    const required = async path => {
      const response = await request(path);
      if (!response.ok) throw new Error(response.error || 'Could not load the dashboard.');
      return response.data;
    };
    const optional = async (path, section, fallback, isRole = false) => {
      const response = await request(path);
      if (response.ok) return response.data;
      if (response.status === 401 || (this.identifier && [403, 404].includes(response.status))) {
        throw new Error(response.error || 'You no longer have access to this project.');
      }
      sectionErrors[section] = response.error || `Could not load ${section}.`;
      return isRole && [403, 404].includes(response.status) ? null : fallback;
    };
    this.publish(initial
      ? {status: 'loading', model: null, error: '', sectionErrors: {}, refreshing: false}
      : {...this.state, refreshing: true, error: ''});
    try {
      const projects = await required('/projects');
      if (!current()) return;
      let model;
      if (this.identifier) {
        const project = projects.find(project => project.identifier.toUpperCase() === this.identifier.toUpperCase());
        if (!project) throw new Error(`Project ${this.identifier.toUpperCase()} not found`);
        const [counts, issues, feed, role] = await Promise.all([
          optional(`/projects/${project.id}/issue-counts`, 'metrics', null),
          optional(`/issues?project_id=${project.id}&limit=1000`, 'issues', []),
          optional(`/projects/${project.id}/activity?limit=14&offset=0`, 'activity', {items: []}),
          optional(`/projects/${project.id}/my-role`, 'permissions', TRANSIENT_ROLE, true),
        ]);
        model = overviewModel({project, counts, issues, activity: feed.items, role}, this.env.now());
      } else {
        const [groups, active, todo, pages] = await Promise.all([
          optional('/project-groups', 'groups', []),
          optional('/issues?status=active&limit=200', 'active issues', []),
          optional('/issues?status=todo&limit=200', 'todo issues', []),
          optional('/pages', 'pages', []),
        ]);
        if (!current()) return;
        const issues = [...active, ...todo];
        const digestIds = topProjectIds(issues, projects);
        const quickProjectId = digestIds[0] ?? projects[0]?.id ?? null;
        const [feeds, role] = await Promise.all([
          Promise.all(digestIds.map(id => optional(`/projects/${id}/activity?limit=8&offset=0`, 'activity', {items: []}))),
          quickProjectId === null ? null : optional(`/projects/${quickProjectId}/my-role`, 'permissions', TRANSIENT_ROLE, true),
        ]);
        model = homeModel({projects, issues, pages, groups, activity: feeds.flatMap(feed => feed.items),
          recents: this.env.recents(), quickProjectId, role});
      }
      if (current()) this.publish({status: 'ready', model, error: '', sectionErrors, refreshing: false});
    } catch (error) {
      if (current()) this.publish({status: 'error', model: null, error: error.message, sectionErrors: {}, refreshing: false});
    }
    if (current()) this.abort = null;
  }

  handleEvent(event) {
    if (this.disposed || this.env.identity() === null) return;
    if (event.type === 'resync.required') {
      this.counter.reset();
      this.activityReady = false;
      void this.load(true);
      return;
    }
    if (event.type === 'activity.baseline') {
      if (Number.isSafeInteger(event.day_count) && event.day_count >= 0) {
        this.counter.seed(event.day_count, this.env.now());
        this.activityReady = true;
        this.env.onChange?.(this.state);
      }
      return;
    }
    if (/^(project|issue)\.(created|updated|deleted|linked|unlinked)$/.test(event.type)) {
      this.counter.record(this.env.now());
      this.env.onChange?.(this.state);
    }
    const relevant = event.type.startsWith('project.')
      || /^(issue|page|comment)\./.test(event.type) || event.type === 'sync_required';
    const projectId = this.state.model?.project?.id;
    const inScope = !this.identifier || event.project_id == null || event.project_id === projectId;
    if (relevant && inScope) {
      const now = this.env.now();
      this.firstInvalidation ??= now;
      this.env.cancel(this.timer);
      this.timer = this.env.delay(() => {this.timer = null; return this.load(false);},
        Math.max(0, Math.min(750, 5000 - (now - this.firstInvalidation))));
    }
  }

  accountChanged() {
    this.generation++;
    this.abort?.abort();
    this.abort = null;
    this.env.cancel(this.timer);
    this.timer = null;
    this.firstInvalidation = null;
    this.counter.reset();
    this.activityReady = false;
    this.publish({status: 'idle', model: null, error: '', sectionErrors: {}, refreshing: false});
  }

  dispose() {
    this.disposed = true;
    this.generation++;
    this.abort?.abort();
    this.env.cancel(this.timer);
  }
}

// Lucide vector markup from the original frontend; strings contain no user data.
const ICONS = {"sun": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"4\"/><path d=\"M12 2v2\"/><path d=\"M12 20v2\"/><path d=\"m4.93 4.93 1.41 1.41\"/><path d=\"m17.66 17.66 1.41 1.41\"/><path d=\"M2 12h2\"/><path d=\"M20 12h2\"/><path d=\"m6.34 17.66-1.41 1.41\"/><path d=\"m19.07 4.93-1.41 1.41\"/></svg>", "sunrise": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M12 2v8\"/><path d=\"m4.93 10.93 1.41 1.41\"/><path d=\"M2 18h2\"/><path d=\"M20 18h2\"/><path d=\"m19.07 10.93-1.41 1.41\"/><path d=\"M22 22H2\"/><path d=\"m8 6 4-4 4 4\"/><path d=\"M16 18a4 4 0 0 0-8 0\"/></svg>", "sunset": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M12 10V2\"/><path d=\"m4.93 10.93 1.41 1.41\"/><path d=\"M2 18h2\"/><path d=\"M20 18h2\"/><path d=\"m19.07 10.93-1.41 1.41\"/><path d=\"M22 22H2\"/><path d=\"m16 6-4 4-4-4\"/><path d=\"M16 18a4 4 0 0 0-8 0\"/></svg>", "moon": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401\"/></svg>", "plus": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M5 12h14\"/><path d=\"M12 5v14\"/></svg>", "command": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M15 6v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3\"/></svg>", "circle": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"10\"/></svg>", "circle-dot": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"10\"/><circle cx=\"12\" cy=\"12\" r=\"1\"/></svg>", "circle-dashed": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M10.1 2.182a10 10 0 0 1 3.8 0\"/><path d=\"M13.9 21.818a10 10 0 0 1-3.8 0\"/><path d=\"M17.609 3.721a10 10 0 0 1 2.69 2.7\"/><path d=\"M2.182 13.9a10 10 0 0 1 0-3.8\"/><path d=\"M20.279 17.609a10 10 0 0 1-2.7 2.69\"/><path d=\"M21.818 10.1a10 10 0 0 1 0 3.8\"/><path d=\"M3.721 6.391a10 10 0 0 1 2.7-2.69\"/><path d=\"M6.391 20.279a10 10 0 0 1-2.69-2.7\"/></svg>", "circle-check-big": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M21.801 10A10 10 0 1 1 17 3.335\"/><path d=\"m9 11 3 3L22 4\"/></svg>", "circle-x": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"m15 9-6 6\"/><path d=\"m9 9 6 6\"/></svg>", "history": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8\"/><path d=\"M3 3v5h5\"/><path d=\"M12 7v5l4 2\"/></svg>", "pin": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M12 17v5\"/><path d=\"M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z\"/></svg>", "file-text": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z\"/><path d=\"M14 2v5a1 1 0 0 0 1 1h5\"/><path d=\"M10 9H8\"/><path d=\"M16 13H8\"/><path d=\"M16 17H8\"/></svg>", "list-checks": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M13 5h8\"/><path d=\"M13 12h8\"/><path d=\"M13 19h8\"/><path d=\"m3 17 2 2 4-4\"/><path d=\"m3 7 2 2 4-4\"/></svg>", "arrow-up-right": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M7 7h10v10\"/><path d=\"M7 17 17 7\"/></svg>"};
function icon(doc, name, className = 'tc-dashboard__icon') {
  const node = element(doc, 'span', null, className);
  node.setAttribute('aria-hidden', 'true');
  node.innerHTML = ICONS[name];
  return node;
}

function element(doc, tag, text = null, className = null) {
  const node = doc.createElement(tag);
  if (text !== null) node.textContent = String(text);
  if (className) node.className = className;
  return node;
}

function link(doc, text, href, className = 'tc-dashboard__link') {
  const node = element(doc, 'a', text, className);
  node.href = doc.defaultView?.LificTopcoatRouting?.href(href)
    ?? `${doc.body?.dataset.lificBasePath ?? ''}${href}`;
  node.dataset.dashboardFocus = href;
  return node;
}

function card(doc, title) {
  const section = element(doc, 'section', null, 'tc-dashboard__card');
  section.append(element(doc, 'h2', title));
  return section;
}

function issueRow(doc, project, issue) {
  const row = link(doc, '', projectRoute(project, 'issues', issue.identifier), 'tc-dashboard__issue');
  const status = icon(doc, {active: 'circle-dot', todo: 'circle', backlog: 'circle-dashed', done: 'circle-check-big', cancelled: 'circle-x'}[issue.status] || 'circle', 'tc-dashboard__status');
  status.dataset.status = issue.status;
  status.removeAttribute('aria-hidden');
  status.setAttribute('role', 'img');
  status.setAttribute('aria-label', issue.status);
  const identifier = element(doc, 'span', issue.identifier, 'tc-dashboard__identifier');
  const title = element(doc, 'span', issue.title, 'tc-dashboard__issue-title');
  const priority = element(doc, 'span', null, 'tc-dashboard__priority');
  priority.dataset.priority = issue.priority;
  priority.setAttribute('role', 'img');
  priority.setAttribute('aria-label', issue.priority);
  priority.title = issue.priority;
  const priorityLines = {high: [12, 6, 18], medium: [9, 15], low: [12]}[issue.priority];
  if (priorityLines) priority.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">'
    + priorityLines.map(y => `<line x1="5" y1="${y}" x2="19" y2="${y}"/>`).join('') + '</svg>';
  else if (issue.priority === 'urgent') priority.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/></svg>';
  row.append(status, identifier, title, priority);
  if (issue.ageLabel) {
    row.dataset.heat = issue.heat;
    row.append(element(doc, 'span', `open ${issue.ageLabel}`, 'tc-dashboard__age'));
    if (issue.idleLabel) row.append(element(doc, 'span', `idle ${issue.idleLabel}`, 'tc-dashboard__idle'));
  }
  return row;
}

function activityText(row) {
  const actor = row.actor_display_name || row.actor_username || (row.actor_is_bot ? 'a bot' : 'system');
  const verbs = {create: row.entity_type === 'comment' ? 'commented on' : `created ${row.entity_type}`,
    delete: row.entity_type === 'comment' ? 'deleted a comment on' : `deleted ${row.entity_type}`,
    update: row.entity_type === 'comment' ? 'edited a comment on' : row.field ? `changed ${row.field} on` : 'updated',
    attach: 'labeled', detach: 'unlabeled', link: 'linked', unlink: 'unlinked'};
  return `${actor} ${verbs[row.action] || row.action || 'updated'} ${row.entity_label || row.entity_type || ''}`;
}

function activityCard(doc, activity) {
  const section = card(doc, 'Recent activity');
  const rate = element(doc, 'span', null, 'tc-dashboard__rate');
  rate.dataset.dashboardActivityRate = '';
  rate.hidden = true;
  section.firstElementChild.append(rate);
  const list = element(doc, 'ul', null, 'tc-dashboard__activity');
  for (const row of activity) {
    const item = element(doc, 'li');
    const content = row.href ? link(doc, '', row.href) : element(doc, 'span', null, 'tc-dashboard__link');
    const text = activityText(row);
    const label = row.entity_label || row.entity_type || '';
    const actor = row.actor_display_name || row.actor_username || (row.actor_is_bot ? 'a bot' : 'system');
    content.append(element(doc, 'span', actor, 'tc-dashboard__actor'),
      doc.createTextNode(text.slice(actor.length, label ? -label.length : undefined)),
      element(doc, 'span', label, 'tc-dashboard__activity-identifier'));
    item.append(content);
    list.append(item);
  }
  section.append(list);
  return section;
}

function renderHome(doc, model, user, now) {
  const fragment = doc.createDocumentFragment();
  const hero = element(doc, 'header', null, 'tc-dashboard__hero');
  const hour = new Date(now).getHours();
  const greeting = hour < 5 ? 'Good night' : hour < 12 ? 'Good morning' : hour < 17 ? 'Good afternoon' : hour < 21 ? 'Good evening' : 'Good night';
  const welcome = element(doc, 'div');
  const heading = element(doc, 'h1', `${greeting}${user ? `, ${user.display_name || user.username}` : ''}`);
  heading.tabIndex = -1;
  heading.dataset.dashboardFocus = 'heading';
  welcome.append(heading, element(doc, 'p', new Date(now).toLocaleDateString('en-US', {weekday: 'long', month: 'long', day: 'numeric'})));
  const actions = element(doc, 'div', null, 'tc-dashboard__actions');
  if (model.newIssueHref) {
    const create = link(doc, 'New issue', model.newIssueHref, 'tc-button');
    create.prepend(icon(doc, 'plus'));
    actions.append(create);
  }
  const jump = element(doc, 'button', 'Jump to…', 'tc-button');
  jump.prepend(icon(doc, 'command'));
  jump.type = 'button';
  jump.dataset.paletteOpen = '';
  jump.dataset.dashboardFocus = 'palette';
  actions.append(jump);
  welcome.prepend(icon(doc, hour < 5 || hour >= 21 ? 'moon' : hour < 12 ? 'sunrise' : hour < 17 ? 'sun' : 'sunset', 'tc-dashboard__greeting-icon'));
  hero.append(welcome, actions);
  const columns = element(doc, 'div', null, 'tc-dashboard__columns');
  const main = element(doc, 'div', null, 'tc-dashboard__main');
  const activeHeading = element(doc, 'h2', 'My active issues');
  activeHeading.append(element(doc, 'span', model.issueTotal, 'tc-dashboard__count'));
  main.append(activeHeading);
  if (!model.issueTotal) {
    const quiet = card(doc, 'All quiet here');
    const mascot = element(doc, 'img');
    mascot.src = doc.defaultView?.LificTopcoatRouting?.href('/__topcoat-dashboard-mascot.png')
      ?? `${doc.body?.dataset.lificBasePath ?? ''}/__topcoat-dashboard-mascot.png`;
    mascot.alt = ''; mascot.width = 180;
    const illustration = element(doc, 'span', null, 'tc-dashboard__mascot');
    illustration.setAttribute('aria-hidden', 'true');
    illustration.style.maskImage = `url("${mascot.src}")`;
    quiet.append(illustration, element(doc, 'p', 'Nothing active or todo assigned to you across your projects right now.'));
    main.append(quiet);
  }
  for (const group of model.issueGroups) {
    const section = card(doc, '');
    const destination = link(doc, '', projectRoute(group.project, 'overview'));
    destination.append(element(doc, 'span', group.project.emoji || group.project.identifier.slice(0, 2), 'tc-dashboard__project-icon'),
      element(doc, 'span', group.project.name, 'tc-dashboard__project-name'),
      element(doc, 'span', group.total, 'tc-dashboard__project-count'));
    section.firstElementChild.append(destination);
    for (const issue of group.visible) section.append(issueRow(doc, group.project, issue));
    if (group.total > group.visible.length) section.append(link(doc, `View all ${group.total} in ${group.project.identifier}`, projectRoute(group.project, 'issues')));
    main.append(section);
  }
  const aside = element(doc, 'aside', null, 'tc-dashboard__rail');
  if (model.recents.length) {
    const recent = card(doc, 'Recently viewed');
    for (const entry of model.recents) recent.append(link(doc, `${entry.title} · ${entry.project}`, entry.href));
    aside.append(recent);
  }
  if (model.pinnedPages.length) {
    const pinned = card(doc, 'Pinned pages');
    for (const page of model.pinnedPages) pinned.append(link(doc, page.title, page.href));
    aside.append(pinned);
  }
  if (model.activity.length) aside.append(activityCard(doc, model.activity));
  if (!model.projects.length) {
    const empty = card(doc, 'No projects yet');
    empty.append(element(doc, 'p', 'Create or import a project to start tracking your work.'), link(doc, 'Create project', '/projects/new'), link(doc, 'Import project', '/projects/import'));
    main.append(empty);
  }
  columns.append(main, aside);
  fragment.append(hero, columns);
  return fragment;
}

function renderOverview(doc, model) {
  const fragment = doc.createDocumentFragment();
  const project = model.project;
  const hero = element(doc, 'header', null, 'tc-dashboard__hero');
  const identity = element(doc, 'div');
  identity.dataset.projectIdentity = '';
  const heading = element(doc, 'h1', project.name);
  heading.tabIndex = -1; heading.dataset.dashboardFocus = 'heading';
  identity.append(element(doc, 'p', project.identifier, 'tc-dashboard__identifier'), heading);
  if (project.description) identity.append(element(doc, 'p', project.description));
  const actions = element(doc, 'div', null, 'tc-dashboard__actions');
  if (model.newIssueHref) {
    const create = link(doc, 'New issue', model.newIssueHref, 'tc-button');
    create.prepend(icon(doc, 'plus'));
    actions.append(create);
  }
  if (model.settingsHref) actions.append(link(doc, 'Project settings', model.settingsHref, 'tc-button'));
  hero.append(identity, actions);
  fragment.append(hero);
  if (model.counts) {
    const metrics = element(doc, 'section', null, 'tc-dashboard__metrics');
    metrics.setAttribute('aria-label', 'Project progress');
    const progress = element(doc, 'progress');
    progress.max = model.counts.total || 1;
    progress.value = model.counts.done;
    progress.setAttribute('aria-label', `${model.counts.done} of ${model.counts.total} issues done`);
    metrics.append(progress, element(doc, 'p', `${model.counts.done}/${model.counts.total} done · ${model.completion}%`));
    fragment.append(metrics);
  }
  const attention = card(doc, 'Needs attention');
  if (!model.attention.length) attention.append(element(doc, 'p', 'Nothing needs attention'));
  for (const issue of model.attention) attention.append(issueRow(doc, project, issue));
  if (model.moreCount) attention.append(link(doc, `View ${model.moreCount} more open issues`, projectRoute(project, 'issues')));
  fragment.append(attention);
  if (model.activity.length) fragment.append(activityCard(doc, model.activity));
  const navigation = element(doc, 'nav', null, 'tc-dashboard__actions');
  navigation.setAttribute('aria-label', 'Project sections');
  for (const [label, section] of [['Issues', 'issues'], ['Pages', 'pages'], ['Modules', 'modules'], ['Plans', 'plans'], ['Activity', 'activity'], ['Insights', 'insights']]) {
    navigation.append(link(doc, label, projectRoute(project, section)));
  }
  fragment.append(navigation);
  return fragment;
}

function loadingFrame(doc, overview) {
  const fragment = doc.createDocumentFragment();
  const hero = element(doc, 'header', null, 'tc-dashboard__hero');
  hero.append(element(doc, 'h1', overview ? 'Project overview' : 'Home'));
  const skeleton = element(doc, 'div', null, 'tc-dashboard__skeleton');
  skeleton.setAttribute('aria-hidden', 'true');
  skeleton.append(element(doc, 'div', null, 'tc-dashboard__skeleton-card'),
    element(doc, 'div', null, 'tc-dashboard__skeleton-card'));
  fragment.append(hero, element(doc, 'h2', overview ? 'Needs attention' : 'My active issues'), skeleton);
  return fragment;
}

function readRecents(win) {
  try {
    const entries = JSON.parse(win.localStorage.getItem('lific_recents') || '[]');
    return Array.isArray(entries) ? entries.filter(entry => entry && typeof entry === 'object') : [];
  } catch { return []; }
}

function attach(root, {window: win = globalThis.window, session = win.lificSession} = {}) {
  const doc = root.ownerDocument;
  const content = root.querySelector('[data-dashboard-content]');
  const status = root.querySelector('[data-dashboard-status]');
  const errors = root.querySelector('[data-dashboard-errors]');
  const identity = () => session.state.publicProject === null && session.state.user ? `private:${session.state.user.id}` : null;
  let audience = identity();
  let lastState = null;
  let lastBaseline;
  let disposed = false;
  const updateRate = () => {
    const element = root.querySelector('[data-dashboard-activity-rate]');
    if (element) {
      element.hidden = !controller.activityReady;
      const rate = controller.activityRate();
      element.textContent = ` · ${rate.value} ${rate.unit}`;
    }
  };
  const render = state => {
    if (lastState === state) {updateRate(); return;}
    lastState = state;
    const focused = root.contains(doc.activeElement) ? doc.activeElement.dataset.dashboardFocus : null;
    const loading = state.status === 'loading' || state.status === 'idle' && session.state.loading;
    root.setAttribute('aria-busy', String(loading || state.refreshing));
    status.textContent = loading ? 'Loading your dashboard…' : state.refreshing ? 'Refreshing…' : '';
    errors.replaceChildren();
    for (const [section, message] of Object.entries(state.sectionErrors)) errors.append(element(doc, 'p', `Could not load ${section}: ${message}`));
    if (loading) {
      content.replaceChildren(loadingFrame(doc, root.dataset.topcoatDashboard === 'overview'));
    } else if (state.status === 'ready') {
      content.replaceChildren(root.dataset.topcoatDashboard === 'overview'
        ? renderOverview(doc, state.model) : renderHome(doc, state.model, session.state.user, Date.now()));
      const project = state.model.project;
      if (project) {
        doc.body.dataset.lificProjectId = String(project.id);
        win.lificSync?.setActiveProject?.(project.id);
      }
      updateRate();
    } else if (state.status === 'error') {
      const failed = card(doc, "Couldn't load your dashboard");
      const retry = element(doc, 'button', 'Try again', 'tc-button');
      retry.type = 'button'; retry.dataset.dashboardFocus = 'retry';
      retry.addEventListener('click', () => void controller.load());
      failed.append(element(doc, 'p', state.error), retry);
      content.replaceChildren(failed);
    } else if (state.status === 'idle') {
      content.replaceChildren();
    }
    if (focused && state.status !== 'loading') {
      const candidates = [...root.querySelectorAll('[data-dashboard-focus]')];
      (candidates.find(element => element.dataset.dashboardFocus === focused)
        || candidates.find(element => element.dataset.dashboardFocus === 'heading'))?.focus();
    }
  };
  const request = (path, options) => {
    const role = path.match(/^\/projects\/(\d+)\/my-role$/);
    // The model and shared affordances consume the same fresh role response.
    return role ? session.loadRole(Number(role[1]), true) : session.request(path, options);
  };
  const controller = new DashboardController({request, identity,
    user: () => session.state.user, recents: () => readRecents(win), onChange: render,
    now: () => Date.now(), delay: win.setTimeout.bind(win), cancel: win.clearTimeout.bind(win)},
  root.dataset.projectIdentifier || null);
  const syncBaseline = () => {
    const baseline = win.lificSync?.state.activityBaseline;
    if (baseline !== lastBaseline && Number.isSafeInteger(baseline) && baseline >= 0) {
      lastBaseline = baseline;
      controller.handleEvent({type: 'activity.baseline', day_count: baseline});
    } else if (baseline === null && lastBaseline !== undefined) {
      lastBaseline = undefined;
      controller.counter.reset();
      controller.activityReady = false;
      updateRate();
    }
  };
  const listeners = [];
  const listen = (target, type, callback) => {
    target.addEventListener(type, callback);
    listeners.push(() => target.removeEventListener(type, callback));
  };
  const transition = () => {
    const next = identity();
    if (next !== audience) {
      audience = next;
      lastBaseline = undefined;
      controller.accountChanged();
      if (next !== null) void controller.load();
    }
  };
  for (const name of ['lific:session-change', 'lific:account-change', 'lific:scope-change']) listen(win, name, transition);
  listen(win, 'lific:realtime', event => controller.handleEvent(event.detail));
  listen(win, 'lific:sync-change', syncBaseline);
  for (const name of ['focus', 'online']) listen(win, name, () => {if (identity() !== null) void controller.load(false);});
  listen(doc, 'visibilitychange', () => {if (!doc.hidden && identity() !== null) void controller.load(false);});
  listen(win, 'storage', event => {
    if (event.key === 'lific_recents' && identity() !== null) void controller.load(false);
    if (event.key === 'lific_token' || event.key === null) transition();
  });
  listen(win, 'lific:project-catalog', () => {if (identity() !== null) void controller.load(false);});
  listen(win, 'pagehide', event => {
    if (event.persisted) controller.accountChanged();
    else controller.dispose();
  });
  listen(win, 'pageshow', event => {
    if (event.persisted) {
      audience = identity();
      lastBaseline = undefined;
      syncBaseline();
      void controller.load();
    }
  });
  const tick = win.setInterval(updateRate, 1000);
  syncBaseline();
  void controller.load();
  return {controller, refresh: () => controller.load(false), dispose() {
    if (disposed) return;
    disposed = true; controller.dispose(); win.clearInterval(tick); listeners.forEach(remove => remove());
  }};
}

const exportsForTests = {homeModel, overviewModel, topProjectIds, activityHref, createActivityCounter, DashboardController, attach, readRecents};
if (typeof module !== 'undefined') module.exports = exportsForTests;
globalThis.LificTopcoatDashboard = exportsForTests;
if (typeof window !== 'undefined') {
  const root = document.querySelector('[data-topcoat-dashboard]');
  if (root && window.lificSession) window.lificDashboard = attach(root);
}
})();
