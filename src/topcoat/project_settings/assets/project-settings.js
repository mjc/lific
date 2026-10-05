(() => {
  "use strict";

  function archiveError(file, limit) {
    return !file || !file.name.toLowerCase().endsWith(".tar.gz")
      ? "Choose a Lific project archive ending in .tar.gz."
      : !file.size
        ? "This archive is empty."
        : file.size > limit
          ? "This archive exceeds the web upload limit. Use the CLI for larger archives."
          : "";
  }

  function initialState() {
    return {
      status: "idle",
      capabilities: null,
      archive: { phase: "idle", progress: null, result: null, error: "" },
      error: "",
      busy: false,
    };
  }

  class ArchiveImportController {
    constructor(env) {
      this.env = env;
      this.state = initialState();
      this.generation = 0;
      this.disposed = false;
    }

    publish(patch) {
      this.state = { ...this.state, ...patch };
      this.env.onChange?.(this.state);
    }

    archiveKey() {
      return "lific:topcoat:archive-pending:" + this.env.identity();
    }

    pendingArchive(value) {
      try {
        value
          ? this.env.storage?.setItem(this.archiveKey(), "1")
          : this.env.storage?.removeItem(this.archiveKey());
      } catch {}
    }

    accountChanged() {
      this.generation++;
      this.publish(initialState());
      if (this.env.identity() !== null) void this.load();
    }

    dispose() {
      this.disposed = true;
      this.generation++;
    }

    async request(path) {
      try {
        return await this.env.request(path);
      } catch (error) {
        return { ok: false, status: null, error: error.message };
      }
    }

    async load() {
      const identity = this.env.identity();
      const generation = ++this.generation;
      const current = () =>
        !this.disposed &&
        generation === this.generation &&
        identity === this.env.identity();
      if (identity === null) {
        this.publish({ ...initialState(), status: "idle" });
        return;
      }

      this.publish({ ...initialState(), status: "loading" });
      const result = await this.request("/project-archives");
      if (!current()) return;
      if (!result.ok) {
        this.publish({ status: "error", error: result.error });
        return;
      }

      let archive = this.state.archive;
      try {
        if (this.env.storage?.getItem(this.archiveKey()))
          archive = {
            phase: "unknown",
            progress: null,
            result: null,
            error:
              "The previous import outcome is unknown. Check the project list before importing again.",
          };
      } catch {}
      this.publish({ status: "ready", capabilities: result.data, archive });
    }

    acknowledgeArchive() {
      if (this.state.busy) return;
      this.pendingArchive(false);
      this.publish({
        archive: { phase: "idle", progress: null, result: null, error: "" },
      });
    }

    async importArchive(file, confirmed) {
      const capabilities = this.state.capabilities;
      if (
        !capabilities?.can_import ||
        !confirmed ||
        ["unknown", "success"].includes(this.state.archive.phase) ||
        this.state.busy
      )
        return false;

      const identity = this.env.identity();
      const generation = this.generation;
      const current = () =>
        !this.disposed &&
        generation === this.generation &&
        identity === this.env.identity();
      const error = archiveError(file, capabilities.max_upload_bytes);
      if (error) {
        this.publish({ error });
        return false;
      }

      this.pendingArchive(true);
      this.publish({
        busy: true,
        error: "",
        archive: { phase: "uploading", progress: 0, result: null, error: "" },
      });
      let result;
      try {
        result = await this.env.upload(
          file,
          (progress) => {
            if (current())
              this.publish({ archive: { ...this.state.archive, progress } });
          },
          () => {
            if (current())
              this.publish({
                archive: { ...this.state.archive, phase: "processing" },
              });
          },
        );
      } catch (failure) {
        result = { ok: false, status: null, error: failure.message };
      }
      if (!current()) return false;

      if (result.ok) {
        this.pendingArchive(false);
        this.publish({
          archive: {
            phase: "success",
            progress: null,
            result: result.data,
            error: "",
          },
        });
      } else {
        const unknown = result.status == null;
        if (!unknown) this.pendingArchive(false);
        this.publish({
          archive: {
            phase: unknown ? "unknown" : "error",
            progress: null,
            result: null,
            error: unknown
              ? `${result.error} Check the project list before importing again.`
              : result.error,
          },
        });
      }
      this.publish({ busy: false });
      return result.ok;
    }
  }

  function node(doc, tag, text = null, attrs = {}) {
    const element = doc.createElement(tag);
    if (text !== null) element.textContent = text;
    for (const [key, value] of Object.entries(attrs)) {
      if (value !== false && value !== null && value !== undefined)
        element.setAttribute(key, value === true ? "" : String(value));
    }
    return element;
  }

  function button(doc, text, action) {
    const element = node(doc, "button", text, { type: "button", class: "tc-button" });
    element.addEventListener("click", action);
    return element;
  }

  function link(doc, text, href) {
    return node(doc, "a", text, { href: window.LificTopcoatRouting?.href(href) ?? href });
  }

  function renderArchive(doc, state, controller) {
    const fragment = doc.createDocumentFragment();
    const archive = state.archive;
    if (!state.capabilities?.can_import) {
      fragment.append(
        node(doc, "p", "Only a signed-in instance admin can import a project archive."),
      );
      return fragment;
    }

    if (archive.phase === "unknown") {
      fragment.append(
        node(doc, "p", archive.error, { role: "alert" }),
        link(doc, "Check project list", "/projects"),
        button(doc, "I've checked the project list", () => controller.acknowledgeArchive()),
      );
      return fragment;
    }

    if (archive.phase === "success") {
      const result = archive.result;
      fragment.append(
        node(doc, "h2", `${result.project.identifier} imported`),
        node(
          doc,
          "p",
          "The new project is private. You are its lead. The source project is unchanged.",
        ),
      );
      const report = node(doc, "dl");
      for (const [table, count] of Object.entries(result.report.rows))
        report.append(
          node(doc, "dt", table.replaceAll("_", " ")),
          node(doc, "dd", String(count)),
        );
      fragment.append(
        report,
        node(doc, "p", `${result.report.blobs} files imported.`),
        node(doc, "h3", `${result.report.external_reference_count} unresolved references`),
      );
      const refs = node(doc, "ul");
      for (const reference of result.report.external_references)
        refs.append(node(doc, "li", reference));
      fragment.append(
        refs,
        link(doc, "Open imported project", `/${encodeURIComponent(result.project.identifier)}/overview`),
        button(doc, "Import another archive", () => controller.acknowledgeArchive()),
      );
      return fragment;
    }

    fragment.append(
      node(
        doc,
        "p",
        "Create a private project from a Lific archive. You become its lead. Accounts and permissions do not transfer; imported author names are text only.",
      ),
      node(doc, "p", "The source stays untouched. Import never merges or overwrites a project."),
    );
    const form = node(doc, "form", null, { "data-form": "archive" });
    const fileLabel = node(doc, "label", "Project archive (.tar.gz)");
    const file = node(doc, "input", null, {
      type: "file",
      name: "archive",
      required: true,
      accept: ".tar.gz,application/gzip",
    });
    fileLabel.append(file);
    const confirmLabel = node(
      doc,
      "label",
      "I understand this imports linked files, history and deleted content that may contain sensitive information.",
      { class: "tc-project-settings__check" },
    );
    const confirmed = node(doc, "input", null, { type: "checkbox", name: "confirmed", required: true });
    confirmLabel.prepend(confirmed);
    const limit = Math.round(state.capabilities.max_upload_bytes / 1024 / 1024);
    const validation = node(doc, "p", "", { role: "alert" });
    form.append(
      fileLabel,
      confirmLabel,
      node(
        doc,
        "p",
        `Web upload limit: ${limit} MiB. Once processing starts, import cannot be canceled. If the connection is lost, check the project list before importing again.`,
      ),
      validation,
    );
    file.addEventListener("change", () => {
      confirmed.checked = false;
      validation.textContent = archiveError(
        file.files[0],
        state.capabilities.max_upload_bytes,
      );
    });
    if (archive.error) form.append(node(doc, "p", archive.error, { role: "alert" }));
    const submit = node(doc, "button", "Import as private project", { type: "submit", class: "tc-button" });
    submit.disabled = state.busy;
    form.append(submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      void controller.importArchive(file.files[0], confirmed.checked);
    });
    fragment.append(form);
    return fragment;
  }

  function attach(root, options = {}) {
    const doc = root.ownerDocument;
    const win = options.win || doc.defaultView;
    const session = options.session || win.lificSession;
    const identity = () =>
      session.state.publicProject === null && session.state.user
        ? `private:${session.state.user.id}`
        : null;
    let token;
    try {
      token = win.sessionStorage;
    } catch {
      token = null;
    }
    const env = {
      identity,
      storage: token,
      request: (path) => session.request(path),
      upload: (file, onProgress, onProcessing) =>
        new Promise((resolve) => {
          const xhr = new win.XMLHttpRequest();
          const authToken = win.localStorage.getItem("lific_token");
          const owner = identity();
          const path = "/api/project-archives";
          xhr.open("POST", win.LificTopcoatRouting?.href(path) ?? path);
          if (authToken) xhr.setRequestHeader("Authorization", `Bearer ${authToken}`);
          xhr.upload.onprogress = (event) =>
            onProgress(
              event.lengthComputable
                ? Math.min(100, Math.round((event.loaded / event.total) * 100))
                : null,
            );
          xhr.upload.onload = onProcessing;
          const unknown = () =>
            resolve({ ok: false, status: null, error: "The archive import outcome is unknown." });
          xhr.onerror = unknown;
          xhr.onabort = unknown;
          xhr.ontimeout = unknown;
          xhr.onload = () => {
            if (owner !== identity() || authToken !== win.localStorage.getItem("lific_token")) {
              unknown();
              return;
            }
            try {
              const body = JSON.parse(xhr.responseText);
              if (xhr.status === 201 && body.project && body.report)
                resolve({ ok: true, data: body });
              else if (xhr.status >= 400 && xhr.status < 500 && typeof body.error === "string")
                resolve({ ok: false, status: xhr.status, error: body.error });
              else unknown();
            } catch {
              unknown();
            }
          };
          const body = new win.FormData();
          body.append("archive", file, file.name);
          try {
            xhr.send(body);
          } catch {
            unknown();
          }
        }),
    };
    const content = root.querySelector("[data-project-settings-content]");
    const status = root.querySelector("[data-project-settings-status]");
    const error = root.querySelector("[data-project-settings-error]");
    let disposed = false;
    let audience = identity();
    const render = (state) => {
      root.setAttribute("aria-busy", String(state.busy || state.status === "loading"));
      status.textContent =
        state.archive.phase === "uploading"
          ? `Uploading… ${state.archive.progress ?? ""}${state.archive.progress === null ? "" : "%"}`
          : state.archive.phase === "processing"
            ? "Importing…"
            : state.status === "loading"
              ? "Loading project access…"
              : "";
      if (state.archive.phase === "uploading") {
        const progress = node(doc, "progress", null, { max: 100, "aria-label": "Archive upload" });
        if (state.archive.progress !== null) progress.value = state.archive.progress;
        status.append(progress);
      }
      error.textContent = state.error;
      if (state.status === "ready") content.replaceChildren(renderArchive(doc, state, controller));
      else if (state.status === "error")
        content.replaceChildren(button(doc, "Try again", () => void controller.load()));
      else content.replaceChildren();
    };
    env.onChange = render;
    const controller = new ArchiveImportController(env);
    const transition = () => {
      const next = identity();
      if (next !== audience) {
        audience = next;
        controller.accountChanged();
      }
    };
    const listeners = ["lific:session-change", "lific:account-change", "lific:scope-change"].map((name) => {
      win.addEventListener(name, transition);
      return () => win.removeEventListener(name, transition);
    });
    const recovery = () => void controller.load();
    for (const name of ["online", "focus"]) {
      win.addEventListener(name, recovery);
      listeners.push(() => win.removeEventListener(name, recovery));
    }
    const storageChange = (event) => {
      if (event.key === "lific_token" || event.key === null) transition();
    };
    win.addEventListener("storage", storageChange);
    void controller.load();
    return {
      controller,
      refresh: () => controller.load(),
      dispose() {
        if (disposed) return;
        disposed = true;
        controller.dispose();
        listeners.forEach((remove) => remove());
        win.removeEventListener("storage", storageChange);
      },
    };
  }

  const api = { archiveError, ArchiveImportController, attach, mount: attach };
  if (typeof module !== "undefined") module.exports = api;
  globalThis.LificTopcoatProjectSettings = api;
  if (typeof window !== "undefined") {
    const root = document.querySelector("[data-topcoat-project-settings]");
    if (root && window.lificSession) window.lificProjectSettings = attach(root);
  }
})();
