import { ipc } from "../../lib/ipc";
import { useT } from "../../i18n";
import type { AgentCard as Card, AgentKind, AgentRow } from "../../lib/types";
import type { Notch } from "../drop/islandMotion";
import { DOT_COLOR, dotOf } from "./agentIsland";

/** The panel under the wings of the open island (the draft field's frame, spec 2026-11-06). */
const FIELD = { left: 14, top: 64, width: 392, bottom: 14 };

/** Nothing under the camera: the panel starts at least 12 pt below the notch. */
function panelTop(notch: Notch): number {
  return Math.max(FIELD.top, (notch?.height ?? 0) + 12);
}

const TEXT = "#e6e8ea";
const DIM = "#8a9097";

function Panel({ notch, children }: { notch: Notch; children: React.ReactNode }) {
  return (
    <div
      data-agent-panel
      style={{
        position: "absolute",
        left: `${FIELD.left}px`,
        top: `${panelTop(notch)}px`,
        width: `${FIELD.width}px`,
        bottom: `${FIELD.bottom}px`,
        boxSizing: "border-box",
        padding: "10px 12px",
        background: "#1C1C1E",
        border: "1px solid #3A3A3D",
        borderRadius: "14px",
        color: TEXT,
        fontFamily: "var(--font)",
        fontSize: "13px",
        lineHeight: 1.3,
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
  );
}

function Button({ label, onClick, strong = false }: { label: string; onClick: () => void; strong?: boolean }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      style={{
        border: "none",
        borderRadius: "9px",
        padding: "5px 12px",
        fontSize: "12px",
        fontWeight: 600,
        fontFamily: "inherit",
        cursor: "pointer",
        background: strong ? "#e6e8ea" : "#2C2C2E",
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

function Title({ text, color }: { text: string; color?: string }) {
  return (
    <div data-agent-title style={{ display: "flex", alignItems: "center", gap: "7px", fontWeight: 600 }}>
      {color && <span style={{ width: 8, height: 8, borderRadius: 4, background: color, flex: "none" }} />}
      <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{text}</span>
    </div>
  );
}

const Buttons = ({ children }: { children: React.ReactNode }) => (
  <div style={{ display: "flex", gap: "6px", justifyContent: "flex-end", marginTop: "auto" }}>{children}</div>
);

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
      <Panel notch={notch}>
        <div>{`${t("agents.after_pause_prefix")} ${parts.join(", ")}`}</div>
      </Panel>
    );
  }
  const row = rows.find((r) => r.key.agent === card.key.agent && r.key.id === card.key.id);
  const head = `${row?.project ?? ""} · ${name(card.key.agent)}`;
  if (card.kind === "permission") {
    const answer = (choice: "allow" | "always" | "deny") => ipc.agentPermission(card.id, choice).catch(() => {});
    return (
      <Panel notch={notch}>
        <Title text={`${head} ${t("agents.permission")}`} color={DOT_COLOR.waiting} />
        {/* All of it, wrapped and scrollable: nothing the user allows may hide behind «…» (review 2026-11-10). */}
        <div
          data-agent-detail
          style={{
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
            fontSize: "12px",
            lineHeight: 1.35,
            color: TEXT,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
            overflowY: "auto",
            minHeight: 0,
            flex: "1 1 auto",
          }}
        >
          {card.detail || card.tool}
        </div>
        {card.can_always && card.rule && (
          <div data-agent-rule style={{ color: DIM, fontSize: "11px", wordBreak: "break-all", flex: "none" }}>
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
    <Panel notch={notch}>
      <Title text={`${head} ${t(done ? "agents.finished" : "agents.waiting")}`} color={DOT_COLOR[done ? "done" : "waiting"]} />
      {done && row?.summary && <div data-agent-summary style={{ color: TEXT }}>{row.summary}</div>}
      <Buttons>
        <Button label={t("agents.open")} onClick={() => ipc.agentOpen(card.key).catch(() => {})} />
        <Button label={t("agents.ok")} onClick={() => ipc.agentDismiss().catch(() => {})} strong />
      </Buttons>
    </Panel>
  );
}

/** One line in the open island (the dizzy Drop's words). */
export function AgentNote({ text, notch }: { text: string; notch: Notch }) {
  return (
    <Panel notch={notch}>
      <div data-agent-note>{text}</div>
    </Panel>
  );
}

/** Pointer over the ears: one line per agent, the most urgent first; a click opens it. */
export function AgentList({ rows, notch }: { rows: AgentRow[]; notch: Notch }) {
  const t = useT();
  const name = useAgentName();
  return (
    <Panel notch={notch}>
      {rows.slice(0, 3).map((r) => (
        <div
          key={`${r.key.agent}:${r.key.id}`}
          data-agent-row
          onClick={() => ipc.agentOpen(r.key).catch(() => {})}
          style={{ cursor: "pointer", display: "flex", flexDirection: "column", gap: "1px" }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "7px" }}>
            <span style={{ width: 8, height: 8, borderRadius: 4, background: DOT_COLOR[dotOf(r.state)], flex: "none" }} />
            <span style={{ fontWeight: 600 }}>{r.project}</span>
            <span style={{ color: DIM }}>{name(r.agent)}</span>
            {r.steps && <span style={{ color: DIM, marginLeft: "auto" }}>{t("agents.step", { n: r.steps[0], m: r.steps[1] })}</span>}
          </div>
          {r.actions.length > 0 && (
            <div style={{ color: DIM, fontSize: "12px", paddingLeft: "15px", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {r.actions.map((a, i) => (
                <span key={i}>
                  {i > 0 && " · "}
                  <span data-agent-action>{a}</span>
                </span>
              ))}
            </div>
          )}
        </div>
      ))}
    </Panel>
  );
}
