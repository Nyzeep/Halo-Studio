import { useEffect, useState } from "react";
import type { McpConfigPreview, McpServerConfig } from "../../shared/mcp";

export const filesystemMcpServer: McpServerConfig = {
  id: "filesystem",
  displayName: "Filesystem",
  transport: "stdio",
  command: "npx",
  args: ["-y", "@modelcontextprotocol/server-filesystem", "D:\\Halo Studio"],
  env: {
    HALO_SCOPE: "workspace"
  },
  enabled: true,
  targetAgents: ["claude-code", "codex-cli", "opencode", "pi"]
};

export function useMcpPreview(server: McpServerConfig = filesystemMcpServer) {
  const [previews, setPreviews] = useState<McpConfigPreview[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    window.halo.mcp.previewConfig(server).then((result) => {
      if (active) {
        setPreviews(result);
        setLoading(false);
      }
    });

    return () => {
      active = false;
    };
  }, [server]);

  return { previews, loading, server };
}
