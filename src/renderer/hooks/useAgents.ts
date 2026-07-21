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
    } catch {
      setAgents([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let active = true;
    window.halo.agents
      .detectAll()
      .then((result) => {
        if (active) {
          setAgents(result);
        }
      })
      .catch(() => {
        if (active) {
          setAgents([]);
        }
      })
      .finally(() => {
        if (active) {
          setLoading(false);
        }
      });

    return () => {
      active = false;
    };
  }, []);

  return { agents, loading, refreshDiscovery };
}
