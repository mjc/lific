"use client";

import { useEffect, useRef, useState } from "react";

export function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1600);
    } catch {
      // clipboard unavailable — nothing sensible to do
    }
  }

  return (
    <button
      type="button"
      onClick={copy}
      aria-label={copied ? "Copied" : "Copy install command"}
      className={`shrink-0 rounded-md border px-3 py-1.5 font-mono text-caption transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
        copied
          ? "border-success/40 text-success"
          : "border-border bg-bg-subtle text-text-muted hover:border-text-faint hover:text-text"
      }`}
    >
      {copied ? "copied" : "copy"}
    </button>
  );
}
