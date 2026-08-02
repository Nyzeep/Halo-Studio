import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const DECISION_TIMEOUT_MS = 30_000;

/**
 * First-party Halo permission seam for Pi RPC.
 *
 * Pi's default tool execution has no Halo permission policy. Every tool call
 * reaches this handler before execution and is blocked unless Halo confirms
 * the single toolCallId through the RPC extension UI protocol.
 */
export default function haloPermissionGate(pi: ExtensionAPI) {
  pi.on("tool_call", async (event, ctx) => {
    try {
      const message = JSON.stringify({
        toolCallId: event.toolCallId,
        toolName: event.toolName,
      });
      const allowed = await ctx.ui.confirm(
        `Halo permission: ${event.toolName}`,
        message,
        { timeout: DECISION_TIMEOUT_MS },
      );

      if (!allowed) {
        return { block: true, reason: "Halo denied or timed out" };
      }
    } catch {
      return { block: true, reason: "Halo permission extension failed" };
    }
  });
}
