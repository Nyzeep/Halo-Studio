import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const DECISION_TIMEOUT_MS = 30_000;

/**
 * First-party Halo permission gate.
 *
 * Pi's built-in tool execution is not Halo's permission system. Every tool
 * call is intercepted before execution and must receive a single confirmation
 * from the Halo host through the RPC extension UI protocol.
 */
export default function haloPermissionGate(pi: ExtensionAPI) {
  pi.on("tool_call", async (event, ctx) => {
    try {
      const message = JSON.stringify({
        toolCallId: event.toolCallId,
        toolName: event.toolName,
      });
      const allowed = await ctx.ui.confirm(
        "Halo permission decision",
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
