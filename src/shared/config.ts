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
