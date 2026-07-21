import { useCallback, useEffect, useState } from "react";
import type { AgentInfo } from "../../shared/agents";

export function useAgents() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const refreshDiscovery = useCallback(async () => {
    setLoading(true);
    try {
      const result = await window.halo.agents.detectAll();
      setAgents(result);
    } finally {
      setLoading(false);
    }
  }, []);

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

  return { agents, loading, refreshDiscovery };
}
