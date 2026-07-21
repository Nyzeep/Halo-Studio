export interface ConfigWriteRequest {
  targetPath: string;
  nextContent: string;
  reason: string;
}

export interface ConfigWriteResult {
  targetPath: string;
  backupPath: string;
  diff: string;
  wroteAt: string;
}

export interface ConfigRollbackRequest {
  targetPath: string;
  backupPath: string;
}

export interface ConfigRollbackResult {
  targetPath: string;
  backupPath: string;
  restored: boolean;
  restoredAt: string;
}

export interface ConfigBackupEntry {
  targetPath: string;
  backupPath: string;
  size: number;
  createdAt: string;
}

export type ConfigWriteRisk = "low" | "blocked";

export interface RealConfigWritePlanRequest {
  workspaceRoot: string;
  targetPath: string;
  nextContent: string;
  reason: string;
}

export interface RealConfigWritePlan {
  workspaceRoot: string;
  targetPath: string;
  normalizedTargetPath: string;
  nextContent: string;
  reason: string;
  allowed: boolean;
  risk: ConfigWriteRisk;
  confirmationPhrase: string;
  warnings: string[];
}

export interface ConfirmedConfigWriteRequest extends RealConfigWritePlanRequest {
  confirmation: string;
}
