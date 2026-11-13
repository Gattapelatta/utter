import { useCallback, useEffect, useState } from "react";
import { useT } from "../../i18n";
import { ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import { AGENTS_OFF, type AgentLink, type AgentsStatus, type AgentSettings } from "../../lib/types";
import { Switch } from "../ui/Switch";
import { ClaudeIcon, CodexIcon } from "./agentIcons";

const AGENT_NAME = { claude_code: "Claude Code", codex: "Codex" } as const;

/** Where the agents stand (detected, connected, broken), read again after every change. */
function useAgentsStatus(): [AgentsStatus | null, () => void] {
  const [status, setStatus] = useState<AgentsStatus | null>(null);
  const read = useCallback(() => {
    ipc
      .agentsStatus()
      .then((s) => setStatus(s ?? null))
      .catch(() => {});
  }, []);
  useEffect(read, [read]);
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
 * The agents as two columns — mark, name, switch in the brand's colour — on the onboarding step
 * and in the Agents section (spec 2026-11-13 agents-look-and-codex-hooks §3.1). Onboarding shows
 * only the agents found on this Mac.
 */
export function AgentToggles({ onboarding = false }: { onboarding?: boolean }) {
  const t = useT();
  const s = useApp((a) => a.settings);
  const update = useApp((a) => a.update);
  const [status, reread] = useAgentsStatus();
  if (!s) return null;
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
    return <div className={`max-w-[200px] text-center text-[12px] leading-snug ${danger ? "text-danger" : "text-text-secondary"}`}>{text}</div>;
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
        <Switch
          label={AGENT_NAME[agent]}
          checked={agents[agent]}
          disabled={!found && !agents[agent]}
          color={BRAND[agent]}
          knob="#FFFFFF"
          onChange={(v) => set({ [agent]: v })}
        />
        {note(agent, status?.[agent], found)}
      </div>
    );
  };

  return (
    <div className="flex items-start justify-center gap-[72px] py-4">
      {col("claude_code")}
      {col("codex")}
    </div>
  );
}
