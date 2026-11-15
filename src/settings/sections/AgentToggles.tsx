import { useCallback, useEffect, useState } from "react";
import { useT } from "../../i18n";
import { ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import { AGENTS_OFF, type AgentLink, type AgentsStatus, type AgentSettings } from "../../lib/types";
import { Switch } from "../ui/Switch";
import { ClaudeIcon, CodexIcon } from "./agentIcons";

const AGENT_NAME = { claude_code: "Claude Code", codex: "Codex" } as const;

/**
 * Where the agents stand (detected, connected, broken, Codex heard), read again after every
 * change and whenever the window comes back: the user trusts Codex's hooks in another app
 * (final review 2026-11-14).
 */
function useAgentsStatus(): [AgentsStatus | null, () => void] {
  const [status, setStatus] = useState<AgentsStatus | null>(null);
  const read = useCallback(() => {
    ipc
      .agentsStatus()
      .then((s) => setStatus(s ?? null))
      .catch(() => {});
  }, []);
  useEffect(() => {
    read();
    const back = () => document.visibilityState !== "hidden" && read();
    window.addEventListener("focus", back);
    document.addEventListener("visibilitychange", back);
    return () => {
      window.removeEventListener("focus", back);
      document.removeEventListener("visibilitychange", back);
    };
  }, [read]);
  return [status, read];
}

/** Whether anything to connect is on this Mac (the onboarding step is shown only then). */
export function useAgentsDetected(): boolean | null {
  const [status] = useAgentsStatus();
  if (!status) return null;
  return status.detected.claude_code || status.detected.codex;
}

/** The agents' brand colours for their switches (the user's choice 2026-11-13: Codex blue). */
export const BRAND = { claude_code: "#D97757", codex: "#4C6EF5" } as const;
const ICON = { claude_code: ClaudeIcon, codex: CodexIcon } as const;

/**
 * The agents with their marks and brand-coloured switches (spec 2026-11-13
 * agents-look-and-codex-hooks §3.1): two columns on the onboarding step (only the agents found on
 * this Mac), rows in the Agents section (the user 2026-11-15).
 */
export function AgentToggles({ onboarding = false }: { onboarding?: boolean }) {
  const t = useT();
  const s = useApp((a) => a.settings);
  const update = useApp((a) => a.update);
  const [status, reread] = useAgentsStatus();
  // Onboarding shows only the agents found: wait for the answer, or a missing one would flash.
  if (!s || (onboarding && !status)) return null;
  const agents: AgentSettings = s.agents ?? AGENTS_OFF;
  const set = (patch: Partial<AgentSettings>) => update({ agents: { ...agents, ...patch } }).then(reread, reread);

  const note = (agent: keyof typeof AGENT_NAME, link: AgentLink | undefined, found: boolean) => {
    const text = !found
      ? t("agents.missing", { agent: AGENT_NAME[agent] })
      : link?.kind === "broken"
        ? t("agents.broken", { agent: AGENT_NAME[agent] })
        : agent === "codex" && agents.codex && status && !status.codex_heard
          ? t("agents.codex_trust")
          : null;
    if (!text) return null;
    const danger = !found || link?.kind === "broken";
    const place = onboarding ? "max-w-[200px] text-center" : "mt-0.5";
    return <div className={`${place} text-[12px] leading-snug ${danger ? "text-danger" : "text-text-secondary"}`}>{text}</div>;
  };
  const toggle = (agent: keyof typeof AGENT_NAME, found: boolean) => (
    <Switch
      label={AGENT_NAME[agent]}
      checked={agents[agent]}
      disabled={!found && !agents[agent]}
      color={BRAND[agent]}
      knob="#FFFFFF"
      onChange={(v) => set({ [agent]: v })}
    />
  );
  /** Settings (the user 2026-11-15): one under another — mark and name at the left, the switch at the right. */
  const row = (agent: keyof typeof AGENT_NAME) => {
    const found = status ? status.detected[agent] : true;
    const Icon = ICON[agent];
    return (
      <div key={agent} data-agent-row={agent} className="row flex items-center gap-3.5 border-border py-3.5">
        <Icon size={30} />
        <div className="min-w-0 flex-1">
          <div data-agent-name className="text-[15px] font-medium text-text">
            {AGENT_NAME[agent]}
          </div>
          {note(agent, status?.[agent], found)}
        </div>
        {toggle(agent, found)}
      </div>
    );
  };
  const col = (agent: keyof typeof AGENT_NAME) => {
    const found = status ? status.detected[agent] : true;
    if (onboarding && !found) return null;
    const Icon = ICON[agent];
    return (
      <div key={agent} className="flex flex-col items-center gap-2.5">
        <div data-agent-col className="flex flex-col items-center gap-2.5">
          <Icon />
          <div className="text-[15px] font-medium text-text">{AGENT_NAME[agent]}</div>
        </div>
        {toggle(agent, found)}
        {note(agent, status?.[agent], found)}
      </div>
    );
  };

  if (!onboarding) {
    return (
      <>
        {row("claude_code")}
        {row("codex")}
      </>
    );
  }
  return (
    <div className="flex items-start justify-center gap-[72px] py-4">
      {col("claude_code")}
      {col("codex")}
    </div>
  );
}
