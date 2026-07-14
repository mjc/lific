"use client";

import { useEffect, useRef, useState } from "react";

/**
 * Autoplay etiquette for the product loops:
 * - plays only while at least a quarter of it is on screen
 * - pauses when scrolled past (battery, data)
 * - under prefers-reduced-motion it never autoplays; the poster shows
 *   and native controls let the visitor opt in
 * - poster + aspect ratio reserve the box, so no layout shift
 */
export function AutoplayVideo({
  src,
  poster,
  aspect,
  label,
}: {
  src: string;
  poster: string;
  /** e.g. "aspect-[1832/860]" matching the rendered mp4 */
  aspect: string;
  label: string;
}) {
  const ref = useRef<HTMLVideoElement>(null);
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(mq.matches);
    if (mq.matches) return;

    const v = ref.current;
    if (!v) return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) v.play().catch(() => {});
          else v.pause();
        }
      },
      { threshold: 0.25 },
    );
    io.observe(v);
    return () => io.disconnect();
  }, []);

  return (
    <video
      ref={ref}
      src={src}
      poster={poster}
      muted
      loop
      playsInline
      preload="none"
      controls={reduced}
      className={`block w-full bg-bg ${aspect}`}
      aria-label={label}
    />
  );
}
