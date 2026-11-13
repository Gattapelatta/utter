import { ipc } from "../../lib/ipc";
import { useT } from "../../i18n";
import type { AgentCard as Card, AgentKind, AgentRow } from "../../lib/types";
import type { Notch } from "../drop/islandMotion";
import { DOT_COLOR, dotOf, ladderOf, type Dot, type Ladder } from "./agentIsland";

/**
 * The agents' open island (the user's mockup 2026-11-09, asked for again 2026-11-10): the big Drop
 * on the left, the words on its right, the island tinted with the state's colour. Nothing under
 * the camera: everything starts 14 pt below the notch.
 */
const TEXT_LEFT = 84;
const TEXT_RIGHT = 18;
const BOTTOM = 14;
const TITLE_H = 18;
const LINE_H = 16;
const BUTTONS_H = 28;
/** Between the lines of the column (flex gap). */
const GAP = 6;
/** Extra air above the buttons. */
const BUTTONS_TOP = 4;
/** The rule line under the command. */
const RULE_H = 14;
/** Characters of the command per line in the mono box (300 pt at 12 px). */
const MONO_PER_LINE = 40;
const MONO_LINES = 3;

const TEXT = "#e6e8ea";
const DIM = "#8a9097";

function top(notch: Notch): number {
  return (notch?.height ?? 0) + 14;
}

/** The Drop (44 pt) must fit too, however short the words. */
function minHeight(notch: Notch): number {
  return top(notch) + 44 + 16;
}

function rgb(hex: string): string {
  const n = parseInt(hex.slice(1), 16);
  return `${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}`;
}

const detailLines = (detail: string) => Math.min(MONO_LINES, Math.max(1, Math.ceil(detail.length / MONO_PER_LINE)));

/** Heights of the column's items, in order (the column flows from the top; the island is cut to fit). */
type Shown = { card: Card } | { rows: AgentRow[] } | { note: true };
const lines = (s: string, per: number, max: number) => Math.min(max, Math.max(1, Math.ceil(s.length / per)));

function items(show: Shown): number[] {
  if ("note" in show) return [2 * LINE_H];
  // The first agent gets its ladder (or two actions), the others one line each (spec 2026-11-13 §3.2).
  if ("rows" in show) return show.rows.length ? show.rows.slice(0, 3).map((r, i) => (i === 0 ? TITLE_H + firstLines(r) * LINE_H : TITLE_H)) : [TITLE_H];
  const c = show.card;
  const buttons = BUTTONS_TOP + BUTTONS_H;
  switch (c.kind) {
    case "after_pause":
      return [2 * LINE_H];
    case "permission":
      return [TITLE_H, detailLines(c.detail || c.tool) * LINE_H + 12, ...(c.can_always && c.rule ? [lines(`«Всегда» разрешит: ${c.rule}`, 52, 3) * RULE_H] : []), buttons];
    case "done":
    case "waiting":
      return [TITLE_H, buttons];
  }
}

/** How tall the island opens for this card or list. */
export function agentHeight(show: Shown, notch: Notch): number {
  const list = items(show);
  const column = list.reduce((s, h) => s + h, 0) + GAP * (list.length - 1);
  return Math.max(minHeight(notch), top(notch) + column + BOTTOM);
}

/** The state's colour washing in from the bottom-right corner (the mockup's «отлив»). */
function Tint({ dot }: { dot: Dot | null }) {
  if (!dot) return null;
  const c = rgb(DOT_COLOR[dot]);
  return (
    <div
      data-agent-tint={dot}
      style={{
        position: "absolute",
        inset: 0,
        pointerEvents: "none",
        background: `radial-gradient(130% 160% at 100% 100%, rgba(${c}, 0.55) 0%, rgba(${c}, 0.18) 45%, rgba(${c}, 0) 75%)`,
      }}
    />
  );
}

/** The state dot on the big Drop's top-left (beside the eye, not over it). */
function DropDot({ dot, notch }: { dot: Dot | null; notch: Notch }) {
  if (!dot) return null;
  return (
    <span
      data-agent-drop-dot={dot}
      style={{
        position: "absolute",
        left: "22px",
        top: `${top(notch) - 2}px`,
        width: 10,
        height: 10,
        borderRadius: 5,
        background: DOT_COLOR[dot],
        boxShadow: "0 0 0 2px #000",
      }}
    />
  );
}

function Panel({ notch, dot, children }: { notch: Notch; dot: Dot | null; children: React.ReactNode }) {
  return (
    <>
      <Tint dot={dot} />
      <DropDot dot={dot} notch={notch} />
      <div
        data-agent-panel
        style={{
          position: "absolute",
          left: `${TEXT_LEFT}px`,
          right: `${TEXT_RIGHT}px`,
          top: `${top(notch)}px`,
          color: TEXT,
          fontFamily: "var(--font)",
          fontSize: "13px",
          lineHeight: `${LINE_H}px`,
          display: "flex",
          flexDirection: "column",
          gap: "6px",
          overflow: "hidden",
          // The clip it sits in lets clicks through; the buttons must take them.
          pointerEvents: "auto",
        }}
      >
        {children}
      </div>
    </>
  );
}

function Button({ label, onClick, strong = false }: { label: string; onClick: () => void; strong?: boolean }) {
  return (
    <button
      type="button"
      data-hit
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      style={{
        border: "none",
        borderRadius: "14px",
        height: `${BUTTONS_H}px`,
        padding: "0 14px",
        fontSize: "12.5px",
        fontWeight: 600,
        fontFamily: "inherit",
        cursor: "pointer",
        background: strong ? "#ffffff" : "rgba(255, 255, 255, 0.12)",
        color: strong ? "#111" : TEXT,
      }}
    >
      {label}
    </button>
  );
}

function useAgentName(): (a: AgentKind) => string {
  const t = useT();
  return (a) => t(a === "codex" ? "agents.codex" : "agents.claude_code");
}

/** «<b>shop</b> Claude Code закончил»: the project bold, the rest dim. */
function Title({ project, rest }: { project: string; rest: string }) {
  return (
    <div data-agent-title style={{ height: `${TITLE_H}px`, lineHeight: `${TITLE_H}px`, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: "none" }}>
      <b style={{ fontWeight: 700, color: TEXT }}>{project}</b> <span style={{ color: DIM }}>{rest}</span>
    </div>
  );
}

const Buttons = ({ children }: { children: React.ReactNode }) => (
  <div style={{ display: "flex", gap: "8px", marginTop: `${BUTTONS_TOP}px`, flex: "none" }}>{children}</div>
);

/** «Правит billing.ts» → «Поправил billing.ts»: a finished step reads in the past. */
const PAST: [RegExp, string][] = [
  [/^Читает /, "Прочитал "],
  [/^Правит /, "Поправил "],
  [/^Запускает /, "Запустил "],
  [/^Ищет /, "Искал "],
  [/^Reading /, "Read "],
  [/^Editing /, "Edited "],
  [/^Running /, "Ran "],
  [/^Searching /, "Searched "],
];
export function pastOf(action: string): string {
  for (const [re, to] of PAST) if (re.test(action)) return action.replace(re, to);
  return action;
}

/** A card opened by an agent: a permission request, «finished», «waits for you», «while paused». */
export function AgentCard({ card, rows, notch }: { card: Card; rows: AgentRow[]; notch: Notch }) {
  const t = useT();
  const name = useAgentName();
  if (card.kind === "after_pause") {
    // «2 агента закончили, 1 ждёт тебя»: plural forms, and no «0 …» parts (spec §3.5).
    const parts = [
      card.done > 0 && t("agents.after_pause_done", { n: card.done }),
      card.waiting > 0 && t("agents.after_pause_waiting", { n: card.waiting }),
    ].filter(Boolean);
    return (
      <Panel notch={notch} dot={card.waiting > 0 ? "waiting" : "done"}>
        <div>{`${t("agents.after_pause_prefix")} ${parts.join(", ")}`}</div>
      </Panel>
    );
  }
  const row = rows.find((r) => r.key.agent === card.key.agent && r.key.id === card.key.id);
  const project = row?.project ?? "";
  if (card.kind === "permission") {
    const answer = (choice: "allow" | "always" | "deny") => ipc.agentPermission(card.id, choice).catch(() => {});
    return (
      <Panel notch={notch} dot="waiting">
        <Title project={project} rest={t("agents.permission")} />
        {/* All of it, wrapped and scrollable: nothing the user allows may hide behind «…» (review 2026-11-10). */}
        <div
          data-agent-detail
          style={{
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
            fontSize: "12px",
            lineHeight: `${LINE_H}px`,
            color: TEXT,
            background: "rgba(255, 255, 255, 0.07)",
            borderRadius: "8px",
            padding: "6px 10px",
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
            overflowY: "auto",
            maxHeight: `${MONO_LINES * LINE_H + 12}px`,
            flex: "0 1 auto",
          }}
        >
          {card.detail || card.tool}
        </div>
        {card.can_always && card.rule && (
          <div data-agent-rule style={{ color: DIM, fontSize: "11px", lineHeight: "14px", wordBreak: "break-all", flex: "none" }}>
            {t("agents.always_rule", { rule: card.rule })}
          </div>
        )}
        <Buttons>
          <Button label={t("agents.deny")} onClick={() => answer("deny")} />
          <Button label={t("agents.allow")} onClick={() => answer("allow")} strong />
          {card.can_always && <Button label={t("agents.always")} onClick={() => answer("always")} />}
        </Buttons>
      </Panel>
    );
  }
  const done = card.kind === "done";
  return (
    <Panel notch={notch} dot={done ? "done" : "waiting"}>
      <Title project={project} rest={`${name(card.key.agent)} ${t(done ? "agents.finished" : "agents.waiting")}`} />
      <Buttons>
        <Button label={t("agents.open")} onClick={() => ipc.agentOpen(card.key).catch(() => {})} strong />
        <Button label={t("agents.ok")} onClick={() => ipc.agentDismiss().catch(() => {})} />
      </Buttons>
    </Panel>
  );
}

/** One line in the open island (the dizzy Drop's words). */
export function AgentNote({ text, notch }: { text: string; notch: Notch }) {
  return (
    <Panel notch={notch} dot={null}>
      <div data-agent-note>{text}</div>
    </Panel>
  );
}

/** Lines under the first agent's title: the ladder's, or (no plan) the last two actions. */
function firstLines(r: AgentRow): number {
  const ladder = ladderOf(r.plan);
  if (!ladder) return Math.min(2, r.actions.length);
  if (ladder.allDone) return 1;
  return [ladder.prev, ladder.current, ladder.next].filter((x) => x !== null).length;
}

/** «Читает event.rs» → «читает event.rs»: the action follows the step after «·». */
const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);

const ONE_LINE = { whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" } as const;

/** The first agent's ladder (spec 2026-11-13 §3.1): the step before, the current one with its live action, the next. */
function LadderLines({ ladder, action }: { ladder: Ladder; action: string | null }) {
  const t = useT();
  if (ladder.allDone) {
    return (
      <div data-agent-step="all-done" style={{ color: DIM, ...ONE_LINE }}>
        {`✓ ${t("agents.all_steps_done", { n: ladder.m })}`}
      </div>
    );
  }
  return (
    <>
      {ladder.prev !== null && (
        <div data-agent-step="prev" style={{ color: DIM, ...ONE_LINE }}>
          {`✓ ${ladder.prev}`}
        </div>
      )}
      {ladder.current !== null && (
        // The step's title matters most (the user 2026-11-13): it keeps up to 70 % of the line, the action gives way first.
        <div data-agent-step="current" style={{ display: "flex", whiteSpace: "nowrap", color: TEXT }}>
          <span style={{ flex: "0 0 auto", maxWidth: "70%", overflow: "hidden", textOverflow: "ellipsis" }}>{`› ${ladder.current}`}</span>
          {action && (
            <span data-agent-action style={{ flex: "0 1 auto", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", color: DIM, whiteSpace: "pre" }}>
              {` · ${lower(action)}`}
            </span>
          )}
        </div>
      )}
      {ladder.next !== null && (
        <div data-agent-step="next" style={{ color: DIM, opacity: 0.6, paddingLeft: "12px", ...ONE_LINE }}>
          {ladder.next}
        </div>
      )}
    </>
  );
}

/** Pointer over the ears: one entry per agent, the most urgent first; a click opens it. */
export function AgentList({ rows, notch }: { rows: AgentRow[]; notch: Notch }) {
  const t = useT();
  const name = useAgentName();
  return (
    <Panel notch={notch} dot={rows[0] ? dotOf(rows[0].state) : null}>
      {rows.length === 0 && <div style={{ color: DIM, lineHeight: `${TITLE_H}px` }}>{t("agents.free")}</div>}
      {rows.slice(0, 3).map((r, i) => {
        const ladder = ladderOf(r.plan);
        const stepOf = (l: Ladder) => t("agents.step", { n: l.n, m: l.m });
        const steps = ladder && !ladder.allDone ? ` · ${stepOf(ladder)}` : "";
        const [older, newest] = r.actions.length > 1 ? [r.actions[r.actions.length - 2], r.actions[r.actions.length - 1]] : [null, r.actions[0] ?? null];
        // Other agents: one line — the step, else the last action, else the agent's name (§3.2 п. 6).
        const tail = ladder
          ? `· ${ladder.allDone ? t("agents.all_steps_done", { n: ladder.m }) : `${stepOf(ladder)} · ${ladder.current}`}`
          : newest
            ? `· ${newest}`
            : name(r.agent);
        return (
          <div
            key={`${r.key.agent}:${r.key.id}`}
            data-agent-row
            data-hit
            onClick={() => ipc.agentOpen(r.key).catch(() => {})}
            style={{ cursor: "pointer", display: "flex", flexDirection: "column", marginTop: i ? `${GAP - 6}px` : 0, flex: "none" }}
          >
            {i === 0 ? (
              <>
                <Title project={r.project} rest={`${name(r.agent)}${steps}`} />
                {ladder ? (
                  <LadderLines ladder={ladder} action={newest} />
                ) : (
                  <>
                    {older && (
                      <div style={{ color: DIM, ...ONE_LINE }}>
                        {"✓ "}
                        <span data-agent-action>{pastOf(older)}</span>
                      </div>
                    )}
                    {newest && (
                      <div style={{ color: TEXT, ...ONE_LINE }}>
                        {"› "}
                        <span data-agent-action>{newest}</span>
                      </div>
                    )}
                  </>
                )}
              </>
            ) : (
              <div style={{ height: `${TITLE_H}px`, display: "flex", alignItems: "center", gap: "6px", whiteSpace: "nowrap", overflow: "hidden" }}>
                <span style={{ width: 7, height: 7, borderRadius: 4, background: DOT_COLOR[dotOf(r.state)], flex: "none" }} />
                <b style={{ fontWeight: 700, flex: "none" }}>{r.project}</b>{" "}
                <span style={{ color: DIM, overflow: "hidden", textOverflow: "ellipsis" }}>{tail}</span>
              </div>
            )}
          </div>
        );
      })}
    </Panel>
  );
}
