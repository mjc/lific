import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  spring,
  interpolate,
} from "remotion";
import { C } from "../theme";
import { BODY, DISPLAY } from "../fonts";
import { Background } from "../components/Background";
import { KineticLine } from "../components/text";

/*
 * The hook: name the category in frame one ("issue trackers"), then set
 * the dilemma the next three scenes prove — the market is either PAID
 * or HEAVY. The reveal ("One binary... Free & open source") resolves it.
 */

const SUB_AT = 26;
const PAID_AT = 46;
const HEAVY_AT = 60;

/** A word that slams in: overshoot scale-down with a hard settle. */
const Slam: React.FC<{ at: number; color: string; children: string }> = ({
  at,
  color,
  children,
}) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({
    frame: frame - at,
    fps,
    config: { damping: 13, stiffness: 220, mass: 0.6 },
  });
  if (frame < at) return null;
  return (
    <span
      style={{
        display: "inline-block",
        fontFamily: DISPLAY,
        fontSize: 110,
        fontWeight: 700,
        letterSpacing: "-0.02em",
        color,
        opacity: Math.min(1, s * 2),
        transform: `scale(${1.9 - s * 0.9})`,
      }}
    >
      {children}
    </span>
  );
};

export const Hook: React.FC = () => {
  const frame = useCurrentFrame();

  const subIn = interpolate(frame, [SUB_AT, SUB_AT + 12], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  // Tiny full-frame shudder on each slam, sells the impact.
  const shake = (at: number) =>
    frame >= at && frame < at + 6
      ? Math.sin((frame - at) * 2.6) * (6 - (frame - at))
      : 0;
  const dy = shake(PAID_AT) + shake(HEAVY_AT);

  return (
    <Background>
      <AbsoluteFill
        style={{
          justifyContent: "center",
          alignItems: "center",
          gap: 46,
          transform: `translateY(${dy}px)`,
        }}
      >
        <KineticLine text="Issue trackers" size={128} />
        <div
          style={{
            fontFamily: BODY,
            fontSize: 42,
            fontWeight: 500,
            color: C.textMuted,
            opacity: subIn,
            transform: `translateY(${(1 - subIn) * 18}px)`,
          }}
        >
          come in two flavors:
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            gap: 90,
            height: 130,
          }}
        >
          <Slam at={PAID_AT} color={C.warn}>
            Paid.
          </Slam>
          <Slam at={HEAVY_AT} color={C.error}>
            Heavy.
          </Slam>
        </div>
      </AbsoluteFill>
    </Background>
  );
};
