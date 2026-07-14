import { CopyButton } from "./components/CopyButton";

const GITHUB = "https://github.com/VoidNullable/lific";
const CRATE = "https://crates.io/crates/lific";
const DISCORD = "https://discord.gg/uWvaFC4f7D";
const RELEASES = "https://github.com/VoidNullable/lific/releases";

const numbers = [
  {
    figure: "29",
    unit: "MCP tools in 6,081 tokens",
    body: "The measured size of the full tools/list response at v2.0.0. Your entire tracker costs about as much context as one long file read. Bloated MCP servers are a real tax — this one isn't.",
  },
  {
    figure: "~25",
    unit: "MB — the whole server",
    body: "Embedded SQLite, embedded web UI, backups built in. No Docker, no Postgres, no reverse proxy, no daemon farm. Copy one binary to a server, point your agents at it, done.",
  },
  {
    figure: "11",
    unit: "AI clients configured by one command",
    body: "lific connect writes correct MCP config into OpenCode, Claude Code, Cursor, VS Code, Codex, Zed, and more. No hand-edited JSON.",
  },
];

const agentAbilities = [
  ["Ask “what can I work on?” in one call", "workable=true returns only issues with every blocker resolved."],
  ["Keep a plan alive across sessions", "persistent, nestable step trees — a fresh session resumes exactly where the last one stopped."],
  ["Break work down and wire it up", "issues, blockers, modules, and plan steps that mirror real issues with two-way done/close sync."],
  ["Read human-friendly identifiers", "APP-42, never a UUID — they survive being spoken, logged, grepped, and pasted into a prompt."],
];

export default function Home() {
  return (
    <div className="flex-1">
      <header className="mx-auto flex w-full max-w-4xl items-baseline justify-between px-6 pt-8">
        <span className="font-serif text-2xl italic tracking-tight">
          Lific
        </span>
        <nav className="flex gap-5 font-mono text-xs text-ink-soft">
          <a className="transition-colors hover:text-accent-deep" href={GITHUB}>
            github
          </a>
          <a className="transition-colors hover:text-accent-deep" href={CRATE}>
            crates.io
          </a>
          <a
            className="transition-colors hover:text-accent-deep"
            href={DISCORD}
          >
            discord
          </a>
        </nav>
      </header>

      <main className="mx-auto w-full max-w-4xl px-6">
        {/* Hero */}
        <section className="pt-[clamp(4rem,12vh,8rem)]">
          <p className="font-mono text-xs uppercase tracking-[0.18em] text-accent-deep">
            Free &amp; open source · Apache-2.0 · self-hosted
          </p>
          <h1 className="mt-5 max-w-[16ch] font-serif text-[clamp(2.75rem,7vw,4.75rem)] leading-[1.02] tracking-tight">
            Issue tracking for the{" "}
            <em className="text-accent-deep">agentic coding era.</em>
          </h1>
          <p className="mt-7 max-w-[52ch] text-lg leading-relaxed text-ink-soft">
            Your agent can write the code. What it can&rsquo;t do is remember:
            the plan dies with the context window and the next session starts
            from zero. Lific is the missing memory — a single-binary issue
            tracker whose primary user is often an agent, not a person.
          </p>

          {/* The install command — the hero object */}
          <div className="mt-12 max-w-2xl">
            <div className="flex items-center justify-between gap-4 rounded-lg border border-line bg-paper-raised py-4 pl-5 pr-3 shadow-[4px_4px_0_0_var(--accent-wash)]">
              <code className="overflow-x-auto whitespace-nowrap font-mono text-[clamp(0.95rem,2.2vw,1.25rem)]">
                <span className="select-none text-ink-faint">$ </span>
                cargo install lific
              </code>
              <CopyButton text="cargo install lific" />
            </div>
            <p className="mt-3 font-mono text-xs text-ink-faint">
              or grab a static binary from{" "}
              <a
                className="underline decoration-line underline-offset-4 transition-colors hover:text-accent-deep hover:decoration-accent"
                href={RELEASES}
              >
                the releases page
              </a>{" "}
              — Linux &amp; macOS, x86_64 &amp; arm64
            </p>
          </div>
        </section>

        {/* Three numbers */}
        <section className="mt-[clamp(6rem,16vh,10rem)]">
          <h2 className="font-serif text-3xl tracking-tight">
            Three numbers instead of adjectives
          </h2>
          <div className="mt-10 space-y-12">
            {numbers.map((n, i) => (
              <div
                key={n.figure}
                className={`flex max-w-3xl flex-col gap-1 border-t border-line-soft pt-6 sm:flex-row sm:gap-10 ${
                  i % 2 === 1 ? "sm:ml-16" : ""
                }`}
              >
                <div className="shrink-0 sm:w-44">
                  <span className="font-serif text-5xl text-accent-deep">
                    {n.figure}
                  </span>
                  <p className="mt-1 text-sm leading-snug text-ink-soft">
                    {n.unit}
                  </p>
                </div>
                <p className="mt-2 leading-relaxed text-ink-soft sm:mt-1">
                  {n.body}
                </p>
              </div>
            ))}
          </div>
        </section>

        {/* 60-second setup */}
        <section className="mt-[clamp(6rem,16vh,10rem)]">
          <div className="flex flex-col gap-10 md:flex-row md:items-start md:gap-14">
            <div className="md:w-72 md:shrink-0">
              <h2 className="font-serif text-3xl tracking-tight">
                The 60-second setup
              </h2>
              <p className="mt-4 leading-relaxed text-ink-soft">
                <code className="font-mono text-sm">init</code> writes config,
                creates the database, prints your API key once, and installs a
                background service — the server survives reboot.{" "}
                <code className="font-mono text-sm">connect</code> detects the
                AI tools on your machine and wires them up. Restart your client
                and the tools are there.
              </p>
            </div>
            <pre className="min-w-0 flex-1 overflow-x-auto rounded-lg bg-term p-6 font-mono text-[13px] leading-loose text-term-ink">
              <code>
                <span className="text-term-green">$</span> cargo install lific
                {"\n"}
                <span className="text-term-green">$</span> lific init{"      "}
                <span className="text-term-faint">
                  # config + db + api key; service on :3456
                </span>
                {"\n"}
                <span className="text-term-green">$</span> lific connect{"   "}
                <span className="text-term-faint">
                  # writes MCP config into your AI clients
                </span>
                {"\n"}
                <span className="text-term-green">$</span> lific doctor{"    "}
                <span className="text-term-faint">
                  # green/yellow/red checks, exits nonzero if broken
                </span>
              </code>
            </pre>
          </div>
        </section>

        {/* What your agent can do */}
        <section className="mt-[clamp(6rem,16vh,10rem)] max-w-3xl">
          <h2 className="font-serif text-3xl tracking-tight">
            What your agent can now do
          </h2>
          <ul className="mt-8">
            {agentAbilities.map(([head, rest]) => (
              <li
                key={head}
                className="border-t border-line-soft py-4 leading-relaxed last:border-b"
              >
                <span className="text-ink">{head}</span>
                <span className="text-ink-faint"> — {rest}</span>
              </li>
            ))}
          </ul>
          <p className="mt-8 text-ink-soft">
            Humans get a full web UI — list, kanban board, pages, dark mode —
            at{" "}
            <code className="font-mono text-sm">localhost:3456</code>.
          </p>
        </section>

        {/* Closing */}
        <section className="mt-[clamp(6rem,16vh,10rem)] pb-8">
          <p className="max-w-[24ch] font-serif text-[clamp(2rem,4.5vw,3rem)] leading-tight tracking-tight">
            Your tracker, your hardware,{" "}
            <em className="text-accent-deep">your data.</em>
          </p>
          <div className="mt-8 flex flex-wrap items-center gap-4">
            <a
              href={GITHUB}
              className="rounded-md bg-accent px-5 py-2.5 font-mono text-sm text-white transition-colors hover:bg-accent-deep"
            >
              Star on GitHub
            </a>
            <code className="font-mono text-sm text-ink-soft">
              <span className="select-none text-ink-faint">$ </span>cargo
              install lific
            </code>
          </div>
        </section>
      </main>

      <footer className="border-t border-line-soft">
        <div className="mx-auto flex w-full max-w-4xl flex-wrap items-baseline justify-between gap-4 px-6 py-8 font-mono text-xs text-ink-faint">
          <span>Apache-2.0 · single binary · no telemetry</span>
          <div className="flex gap-5">
            <a className="transition-colors hover:text-accent-deep" href={GITHUB}>
              github
            </a>
            <a className="transition-colors hover:text-accent-deep" href={CRATE}>
              crates.io
            </a>
            <a
              className="transition-colors hover:text-accent-deep"
              href={DISCORD}
            >
              discord
            </a>
          </div>
        </div>
      </footer>
    </div>
  );
}
