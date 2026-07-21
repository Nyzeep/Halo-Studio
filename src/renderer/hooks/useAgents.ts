import { useEffect, useState } from "react";
import type { AgentInfo } from "../../shared/agents";

export function useAgents() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    window.halo.agents.detectAll().then((result) => {
      if (active) {
        setAgents(result);
        setLoading(false);
      }
    });

    return () => {
      active = false;
    };
  }, []);

  return { agents, loading };
}
