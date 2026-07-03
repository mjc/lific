import React from "react";
import { AbsoluteFill, useCurrentFrame, useVideoConfig, spring } from "remotion";
import { C } from "../theme";
import { BODY, DISPLAY } from "../fonts";
import { Background } from "../components/Background";
import { KineticLine, FadeUp } from "../components/text";

/*
 * Competitor logos: official marks via Simple Icons, used unmodified and
 * purely referentially (nominative fair use in truthful comparative
 * advertising). Brand colors from each company's own palette.
 */
const JIRA_PATH =
  "M11.571 11.513H0a5.218 5.218 0 0 0 5.232 5.215h2.13v2.057A5.215 5.215 0 0 0 12.575 24V12.518a1.005 1.005 0 0 0-1.005-1.005zm5.723-5.756H5.736a5.215 5.215 0 0 0 5.215 5.214h2.129v2.058a5.218 5.218 0 0 0 5.215 5.214V6.758a1.001 1.001 0 0 0-1.001-1.001zM23.013 0H11.455a5.215 5.215 0 0 0 5.215 5.215h2.129v2.057A5.215 5.215 0 0 0 24 12.483V1.005A1.001 1.001 0 0 0 23.013 0Z";
const LINEAR_PATH =
  "M2.886 4.18A11.982 11.982 0 0 1 11.99 0C18.624 0 24 5.376 24 12.009c0 3.64-1.62 6.903-4.18 9.105L2.887 4.18ZM1.817 5.626l16.556 16.556c-.524.33-1.075.62-1.65.866L.951 7.277c.247-.575.537-1.126.866-1.65ZM.322 9.163l14.515 14.515c-.71.172-1.443.282-2.195.322L0 11.358a12 12 0 0 1 .322-2.195Zm-.17 4.862 9.823 9.824a12.02 12.02 0 0 1-9.824-9.824Z";

/** Brand tile: the mark in its brand color, springing in beside the headline. */
const BrandMark: React.FC<{ path: string; color: string; delay?: number }> = ({
  path,
  color,
  delay = 4,
}) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({
    frame: frame - delay,
    fps,
    config: { damping: 14, stiffness: 120, mass: 0.8 },
  });
  return (
    <div
      style={{
        width: 132,
        height: 132,
        borderRadius: 30,
        backgroundColor: C.bgSubtle,
        border: `1px solid ${C.border}`,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transform: `scale(${s})`,
        boxShadow: `0 0 70px ${color}33`,
      }}
    >
      <svg width="76" height="76" viewBox="0 0 24 24">
        <path d={path} fill={color} />
      </svg>
    </div>
  );
};

const FactChip: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <span
    style={{
      fontFamily: BODY,
      fontSize: 31,
      fontWeight: 500,
      color: C.textMuted,
      border: `1px solid ${C.border}`,
      backgroundColor: C.bgSubtle,
      borderRadius: 999,
      padding: "12px 28px",
    }}
  >
    {children}
  </span>
);

/** Beat 2a — Jira. Punching up is a tradition here. */
export const AgitateJira: React.FC = () => (
  <Background glow={false}>
    <AbsoluteFill
      style={{ justifyContent: "center", alignItems: "center", gap: 54 }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 44 }}>
        <BrandMark path={JIRA_PATH} color="#0052CC" />
        <KineticLine text="Why pay for Jira?" size={104} />
      </div>
      <FadeUp delay={15} duration={14}>
        <div style={{ display: "flex", gap: 20 }}>
          <FactChip>$7.91 per user / month</FactChip>
          <FactChip>famously slow</FactChip>
          <FactChip>built for the enterprise, not for you</FactChip>
        </div>
      </FadeUp>
    </AbsoluteFill>
  </Background>
);

/** Beat 2b — Linear. Never attack its quality; only lock-in + price. */
export const AgitateLinear: React.FC = () => (
  <Background glow={false}>
    <AbsoluteFill
      style={{ justifyContent: "center", alignItems: "center", gap: 54 }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 44 }}>
        <BrandMark path={LINEAR_PATH} color="#5E6AD2" />
        <KineticLine text="Why pay for Linear?" size={104} />
      </div>
      <FadeUp delay={15} duration={14}>
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 20,
          }}
        >
          <div style={{ display: "flex", gap: 20 }}>
            <FactChip>beautiful — credit where due</FactChip>
            <FactChip>$10–16 per user / month</FactChip>
          </div>
          <div style={{ display: "flex", gap: 20 }}>
            <FactChip>SaaS-only. Your issues live on their servers.</FactChip>
          </div>
        </div>
      </FadeUp>
    </AbsoluteFill>
  </Background>
);

type FossTool = { name: string; facts: string[] };

// Every number verified against each project's own docs / compose files
// on 2026-07-02. See LIF-DOC-17. Grouped on purpose: no single project
// gets singled out.
const TOOLS: FossTool[] = [
  { name: "Plane", facts: ["13 services", "8 GB RAM recommended"] },
  { name: "Taiga", facts: ["9 containers", "2 RabbitMQ instances"] },
  { name: "Huly", facts: ["14 services", "16 GB RAM recommended"] },
  { name: "OpenProject", facts: ["quad-core + 4 GB minimum", "PostgreSQL 16+"] },
];

/** Beat 2c — the FOSS field, grouped, facts from their own docs. */
export const AgitateFoss: React.FC = () => (
  <Background glow={false}>
    <AbsoluteFill
      style={{ justifyContent: "center", alignItems: "center", gap: 56 }}
    >
      <KineticLine text="Free options get heavy." size={96} />
      <FadeUp delay={12} duration={14}>
        <div style={{ display: "flex", gap: 24 }}>
          {TOOLS.map((tool, i) => (
            <FadeUp key={tool.name} delay={17 + i * 5} duration={14}>
              <div
                style={{
                  width: 330,
                  borderRadius: 14,
                  border: `1px solid ${C.border}`,
                  backgroundColor: C.bgSubtle,
                  padding: "26px 28px",
                  display: "flex",
                  flexDirection: "column",
                  gap: 14,
                }}
              >
                <div
                  style={{
                    fontFamily: DISPLAY,
                    fontSize: 32,
                    fontWeight: 600,
                    color: C.text,
                  }}
                >
                  {tool.name}
                </div>
                {tool.facts.map((fact) => (
                  <div
                    key={fact}
                    style={{
                      fontFamily: BODY,
                      fontSize: 24,
                      color: C.textMuted,
                    }}
                  >
                    {fact}
                  </div>
                ))}
              </div>
            </FadeUp>
          ))}
        </div>
      </FadeUp>
      <FadeUp delay={40} duration={14}>
        <div style={{ fontFamily: BODY, fontSize: 22, color: C.textFaint }}>
          figures from each project&apos;s own documentation
        </div>
      </FadeUp>
    </AbsoluteFill>
  </Background>
);
