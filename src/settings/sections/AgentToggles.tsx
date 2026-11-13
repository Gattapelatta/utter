import { useCallback, useEffect, useState } from "react";
import { useT } from "../../i18n";
import { ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import { AGENTS_OFF, type AgentLink, type AgentsStatus, type AgentSettings } from "../../lib/types";
import { Row } from "../ui/Row";
import { Switch } from "../ui/Switch";

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

/**
 * The switches of the Agents section (spec 2026-11-09 §3.1); the onboarding step shows the
 * agents only, and only those found on this Mac.
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
    const text = !found ? t("agents.missing", { agent: AGENT_NAME[agent] }) : link?.kind === "broken" ? t("agents.broken", { agent: AGENT_NAME[agent] }) : null;
    return text ? <div className="mt-1 text-[13px] leading-snug text-danger">{text}</div> : null;
  };
  const row = (agent: keyof typeof AGENT_NAME, hint: string) => {
    const found = status ? status.detected[agent] : true;
    if (onboarding && !found) return null;
    return (
      <Row label={AGENT_NAME[agent]} description={hint} note={note(agent, status?.[agent], found)}>
        <Switch label={AGENT_NAME[agent]} checked={agents[agent]} disabled={!found && !agents[agent]} onChange={(v) => set({ [agent]: v })} />
      </Row>
    );
  };

  return (
    <>
      {row("claude_code", t("agents.claude_code_hint"))}
      {row("codex", t("agents.codex_hint"))}
    </>
  );
}
