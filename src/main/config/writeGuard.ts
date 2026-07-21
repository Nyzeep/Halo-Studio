import path from "node:path";
import type {
  ConfigWriteResult,
  ConfirmedConfigWriteRequest,
  RealConfigWritePlan,
  RealConfigWritePlanRequest
} from "../../shared/config.js";
import { applyConfigWrite } from "./configFileService.js";

const blockedSegments = new Set([".git", "node_modules", "dist"]);

export function planRealConfigWrite(request: RealConfigWritePlanRequest): RealConfigWritePlan {
  const workspaceRoot = path.resolve(request.workspaceRoot);
  const normalizedTargetPath = path.resolve(request.targetPath);
  const warnings: string[] = [];
  const relativeTarget = path.relative(workspaceRoot, normalizedTargetPath);
  const isInsideWorkspace = relativeTarget === "" || (!relativeTarget.startsWith("..") && !path.isAbsolute(relativeTarget));

  if (!isInsideWorkspace) {
    warnings.push("目标路径不在当前工作区内，已拦截。");
  }

  const segments = relativeTarget.split(path.sep).filter(Boolean);
  const blockedSegment = segments.find((segment) => blockedSegments.has(segment));
  if (blockedSegment) {
    warnings.push(`目标路径位于 ${blockedSegment} 目录内，已拦截。`);
  }

  const allowed = warnings.length === 0;

  return {
    workspaceRoot,
    targetPath: request.targetPath,
    normalizedTargetPath,
    nextContent: request.nextContent,
    reason: request.reason,
    allowed,
    risk: allowed ? "low" : "blocked",
    confirmationPhrase: createConfirmationPhrase(normalizedTargetPath),
    warnings
  };
}

export async function applyConfirmedConfigWrite(request: ConfirmedConfigWriteRequest): Promise<ConfigWriteResult> {
  const plan = planRealConfigWrite(request);
  if (!plan.allowed) {
    throw new Error(plan.warnings.join(" "));
  }

  if (request.confirmation !== plan.confirmationPhrase) {
    throw new Error(`确认短语不匹配，请输入：${plan.confirmationPhrase}`);
  }

  return applyConfigWrite({
    targetPath: plan.normalizedTargetPath,
    nextContent: plan.nextContent,
    reason: plan.reason
  });
}

function createConfirmationPhrase(targetPath: string) {
  return `APPLY ${path.basename(targetPath)}`;
}
