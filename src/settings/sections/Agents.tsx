import { AgentToggles } from "./AgentToggles";
import { Section } from "./Section";

/** «Агенты»: the Drop watches Claude Code and Codex (spec 2026-11-09 §3.1). */
export function Agents() {
  return (
    <Section route="agents">
      <AgentToggles />
    </Section>
  );
}
