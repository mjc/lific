// LIF-268: shared pending-upload state for every markdown composer.
//
// LIF-262 landed the mechanical upload path (drag / paste / button →
// uploadAttachment → insert markdown → toast on failure) in `compose.ts`.
// That file stays the source of truth for the *pure* helpers (markdownFor,
// insertAtCaret, filesFromClipboard/Drop). This module adds the *stateful*
// layer both composers share so they never fork behaviour: a reactive list of
// in-flight uploads, each rendered as a chip in `PendingUploads.svelte`.
//
// A single upload moves through: uploading → (success → inserted, chip drops)
// or (error → chip turns red with the server reason + retry/dismiss). Image
// files carry a `previewUrl` (object URL) revoked the moment the chip leaves,
// so a long-lived composer never leaks blob URLs.
//
// The controller is instantiated once per composer via `createUploadController`
// and exposes an imperative surface (`enqueue`, `retry`, `dismiss`) plus the
// reactive `items` array the strip renders. Because it uses runes it lives in a
// `.svelte.ts` module.

import { uploadAttachment, type AttachmentEntity } from "../api";
import { markdownFor } from "./compose";

export type UploadStatus = "uploading" | "error";

export interface PendingUpload {
  /** Stable client id — chips key on this so retry/dismiss target one row. */
  readonly id: number;
  readonly file: File;
  readonly filename: string;
  readonly size: number;
  readonly isImage: boolean;
  /** Object URL for an image preview thumbnail; null for non-images. Revoked
   *  on removal. */
  readonly previewUrl: string | null;
  status: UploadStatus;
  /** Server-supplied reason, present only in the error state. */
  error: string | null;
}

export interface UploadControllerOptions {
  /** Entity to link finished uploads to, when the parent id is already known
   *  (detail views). Omitted for not-yet-created entities (new-issue form,
   *  new comment) which rely on server re-scan of the saved body. A getter so
   *  the composer can pass a value that becomes known after mount. */
  link?: () => { entity_type: AttachmentEntity; entity_id: number } | null | undefined;
  /** Insert the finished markdown reference at the caret. */
  onInsert: (snippet: string) => void;
}

export interface UploadController {
  /** Reactive list of in-flight / failed uploads for the strip to render. */
  readonly items: PendingUpload[];
  /** True while at least one upload is in flight (drives busy affordances). */
  readonly busy: boolean;
  /** Queue files for upload; images get a preview thumbnail immediately. */
  enqueue: (files: File[]) => void;
  /** Re-attempt a failed upload in place. */
  retry: (id: number) => void;
  /** Drop a chip (typically a failed one) and revoke its preview URL. */
  dismiss: (id: number) => void;
  /** Revoke every outstanding object URL. Call on composer teardown. */
  destroy: () => void;
}

export function createUploadController(opts: UploadControllerOptions): UploadController {
  let seq = 0;
  const items = $state<PendingUpload[]>([]);

  function indexOf(id: number): number {
    return items.findIndex((it) => it.id === id);
  }

  function revoke(item: PendingUpload) {
    if (item.previewUrl) URL.revokeObjectURL(item.previewUrl);
  }

  async function run(item: PendingUpload) {
    const link = opts.link?.() ?? undefined;
    const result = await uploadAttachment(item.file, link ?? undefined);
    const i = indexOf(item.id);
    if (i === -1) return; // dismissed mid-flight
    if (result.ok) {
      opts.onInsert(markdownFor(result.data));
      revoke(items[i]);
      items.splice(i, 1);
    } else {
      items[i].status = "error";
      items[i].error = result.error;
    }
  }

  function enqueue(files: File[]) {
    for (const file of files) {
      const isImage = file.type.startsWith("image/");
      const item: PendingUpload = {
        id: ++seq,
        file,
        filename: file.name,
        size: file.size,
        isImage,
        previewUrl: isImage ? URL.createObjectURL(file) : null,
        status: "uploading",
        error: null,
      };
      items.push(item);
      void run(item);
    }
  }

  function retry(id: number) {
    const i = indexOf(id);
    if (i === -1) return;
    items[i].status = "uploading";
    items[i].error = null;
    void run(items[i]);
  }

  function dismiss(id: number) {
    const i = indexOf(id);
    if (i === -1) return;
    revoke(items[i]);
    items.splice(i, 1);
  }

  function destroy() {
    for (const it of items) revoke(it);
    items.splice(0, items.length);
  }

  return {
    get items() {
      return items;
    },
    get busy() {
      return items.some((it) => it.status === "uploading");
    },
    enqueue,
    retry,
    dismiss,
    destroy,
  };
}
